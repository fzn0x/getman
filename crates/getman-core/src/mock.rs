//! Matching incoming requests to saved examples, for `getman mock`.
//!
//! Every example is a route: the method and URL of the request that got
//! it (the example's own request when it has one, else the request it is
//! saved on), and its response. A route's URL is reduced to its path: the
//! scheme and host, or a leading `{{variable}}` standing for them, are
//! dropped. Path segments written as `:name` or holding a `{{variable}}`
//! match any one segment. The query string is ignored, unless the
//! example's own request has one: then each of its parameters must be
//! present, with the same value unless the value is a `{{variable}}`.
//!
//! When several routes match, the one with the most literal segments and
//! query parameters wins. Among its examples, the one named in the
//! `x-getman-example` header is chosen, or else the first by status code
//! with 2xx first.

use percent_encoding::percent_decode_str;
use serde::Serialize;

use crate::error::Result;
use crate::model::{Example, Request};
use crate::runner::requests_in;
use crate::workspace::Workspace;

/// Header that picks an example by name.
pub const EXAMPLE_HEADER: &str = "x-getman-example";

/// An example and the requests it answers.
#[derive(Debug, Clone)]
pub struct Route {
    /// Path of the request file in the workspace.
    pub request: String,
    /// HTTP method, in capitals.
    pub method: String,
    /// The path matched, as written, such as `/users/:id`.
    pub pattern: String,
    /// The response.
    pub example: Example,
    segments: Vec<Segment>,
    query: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Any,
}

impl Route {
    fn new(request: &str, saved: &Request, example: &Example) -> Self {
        let own = example.request.as_ref();
        let method = own
            .map(|r| r.method.trim())
            .filter(|m| !m.is_empty())
            .unwrap_or(&saved.method)
            .to_ascii_uppercase();
        let own_url = own.map(|r| r.url.trim()).filter(|u| !u.is_empty());
        let (path, query) = split_url(own_url.unwrap_or(&saved.url));
        let segments = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|segment| {
                if segment.starts_with(':') || segment.contains("{{") {
                    Segment::Any
                } else {
                    Segment::Literal(decode(segment))
                }
            })
            .collect();
        // Only an example's own request narrows the query.
        let query = match (own_url, query) {
            (Some(_), Some(query)) => url::form_urlencoded::parse(query.as_bytes())
                .map(|(name, value)| {
                    let any = value.is_empty() || value.contains("{{") || value.starts_with(':');
                    (name.into_owned(), (!any).then(|| value.into_owned()))
                })
                .collect(),
            _ => Vec::new(),
        };
        Self {
            request: request.to_owned(),
            method,
            pattern: if path.is_empty() {
                "/".to_owned()
            } else {
                path.to_owned()
            },
            example: example.clone(),
            segments,
            query,
        }
    }

    /// How specific the route is when it matches, or `None` when not.
    fn score(&self, segments: &[String], query: &[(String, String)]) -> Option<(usize, usize)> {
        if segments.len() != self.segments.len() {
            return None;
        }
        let mut literal = 0;
        for (want, got) in self.segments.iter().zip(segments) {
            match want {
                Segment::Any => {}
                Segment::Literal(text) if text == got => literal += 1,
                Segment::Literal(_) => return None,
            }
        }
        for (name, value) in &self.query {
            let present = query
                .iter()
                .any(|(n, v)| n == name && value.as_ref().is_none_or(|value| value == v));
            if !present {
                return None;
            }
        }
        Some((literal, self.query.len()))
    }
}

/// What an incoming request gets.
#[derive(Debug)]
pub enum Answer<'a> {
    /// This example.
    Example(&'a Route),
    /// Routes matched, but none has an example of the asked name.
    NoSuchExample {
        /// The name asked for.
        name: String,
        /// Names of the examples that matched.
        available: Vec<String>,
    },
    /// No route matched.
    NotFound,
}

/// A request's examples on one method and path, for listing what is
/// mocked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mocked {
    /// HTTP method.
    pub method: String,
    /// Path, as written.
    pub path: String,
    /// Path of the request file.
    pub request: String,
    /// The examples, by name and status.
    pub examples: Vec<MockedExample>,
}

/// An example as listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MockedExample {
    /// Its name.
    pub name: String,
    /// Its status code.
    pub status: u16,
}

/// The examples of a folder, ready to answer requests.
#[derive(Debug, Clone, Default)]
pub struct Routes {
    routes: Vec<Route>,
}

impl Routes {
    /// The examples of every request in a folder; the empty path is the
    /// whole workspace. Requests that cannot be read are skipped.
    pub fn load(workspace: &Workspace, folder: &str) -> Result<Self> {
        let items = requests_in(&workspace.tree()?, folder)?;
        let requests = items
            .into_iter()
            .filter_map(|item| Some((item.path.clone(), workspace.read_request(&item.path).ok()?)));
        Ok(Self::from_requests(requests))
    }

    /// Routes for requests given with their paths.
    pub fn from_requests(requests: impl IntoIterator<Item = (String, Request)>) -> Self {
        let mut routes = Vec::new();
        for (path, request) in requests {
            for example in &request.examples {
                routes.push(Route::new(&path, &request, example));
            }
        }
        Self { routes }
    }

    /// Every route.
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// Whether there is no example to serve.
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// The example for a request. `path` is the request's path and `query`
    /// its query string; `wanted` is the example asked for by name.
    pub fn find(
        &self,
        method: &str,
        path: &str,
        query: Option<&str>,
        wanted: Option<&str>,
    ) -> Answer<'_> {
        let segments: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(decode)
            .collect();
        let query: Vec<(String, String)> =
            url::form_urlencoded::parse(query.unwrap_or_default().as_bytes())
                .into_owned()
                .collect();
        let method = method.to_ascii_uppercase();
        let mut answer = self.find_for(&method, &segments, &query, wanted);
        if matches!(answer, Answer::NotFound) && method == "HEAD" {
            answer = self.find_for("GET", &segments, &query, wanted);
        }
        answer
    }

    fn find_for(
        &self,
        method: &str,
        segments: &[String],
        query: &[(String, String)],
        wanted: Option<&str>,
    ) -> Answer<'_> {
        let scored: Vec<(&Route, (usize, usize))> = self
            .routes
            .iter()
            .filter(|route| route.method == method)
            .filter_map(|route| Some((route, route.score(segments, query)?)))
            .collect();
        let Some(best) = scored.iter().map(|(_, score)| *score).max() else {
            return Answer::NotFound;
        };
        let mut candidates: Vec<&Route> = scored
            .into_iter()
            .filter(|(_, score)| *score == best)
            .map(|(route, _)| route)
            .collect();
        if let Some(name) = wanted.map(str::trim).filter(|n| !n.is_empty()) {
            return candidates
                .iter()
                .find(|route| route.example.name == name)
                .or_else(|| {
                    candidates
                        .iter()
                        .find(|route| route.example.name.eq_ignore_ascii_case(name))
                })
                .map_or_else(
                    || Answer::NoSuchExample {
                        name: name.to_owned(),
                        available: candidates.iter().map(|r| r.example.name.clone()).collect(),
                    },
                    |route| Answer::Example(route),
                );
        }
        candidates.sort_by_key(|route| {
            let status = route.example.status;
            (!(200..300).contains(&status), status)
        });
        Answer::Example(candidates[0])
    }

    /// What is mocked, grouped by request, method and path.
    pub fn summary(&self) -> Vec<Mocked> {
        let mut list: Vec<Mocked> = Vec::new();
        for route in &self.routes {
            let example = MockedExample {
                name: route.example.name.clone(),
                status: route.example.status,
            };
            match list.iter_mut().find(|m| {
                m.request == route.request && m.method == route.method && m.path == route.pattern
            }) {
                Some(entry) => entry.examples.push(example),
                None => list.push(Mocked {
                    method: route.method.clone(),
                    path: route.pattern.clone(),
                    request: route.request.clone(),
                    examples: vec![example],
                }),
            }
        }
        list
    }
}

/// A URL's path and query string, without the scheme and host or the
/// `{{variable}}` that stands for them.
fn split_url(url: &str) -> (&str, Option<&str>) {
    let url = url.split('#').next().unwrap_or_default();
    let (before, query) = match url.split_once('?') {
        Some((before, query)) => (before, Some(query)),
        None => (url, None),
    };
    let mut path = if let Some(at) = before.find("://") {
        let after = &before[at + 3..];
        after.find('/').map_or("", |i| &after[i..])
    } else if let Some(rest) = before.strip_prefix("{{") {
        rest.find("}}").map_or(before, |end| &rest[end + 2..])
    } else {
        before
    };
    if !path.starts_with('/') {
        path = path.find('/').map_or("", |i| &path[i..]);
    }
    (path.trim_end_matches('/'), query)
}

fn decode(segment: &str) -> String {
    percent_decode_str(segment).decode_utf8_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ExampleRequest;

    fn example(name: &str, status: u16) -> Example {
        Example {
            name: name.into(),
            status,
            ..Example::default()
        }
    }

    fn request(method: &str, url: &str, examples: Vec<Example>) -> Request {
        Request {
            method: method.into(),
            url: url.into(),
            examples,
            ..Request::default()
        }
    }

    fn routes() -> Routes {
        Routes::from_requests([
            (
                "users/get.yaml".to_owned(),
                request(
                    "GET",
                    "{{baseUrl}}/users/:id",
                    vec![example("Missing", 404), example("Found", 200)],
                ),
            ),
            (
                "users/me.yaml".to_owned(),
                request(
                    "GET",
                    "https://api.example.com/users/me",
                    vec![example("Me", 200)],
                ),
            ),
            (
                "users/search.yaml".to_owned(),
                request(
                    "GET",
                    "{{baseUrl}}/users?q={{q}}",
                    vec![
                        example("All", 200),
                        Example {
                            request: Some(ExampleRequest {
                                url: "{{baseUrl}}/users?role=admin&q={{q}}".into(),
                                ..ExampleRequest::default()
                            }),
                            ..example("Admins", 200)
                        },
                    ],
                ),
            ),
            (
                "orders/create.yaml".to_owned(),
                request(
                    "POST",
                    "{{host}}/orders/{{shop}}/items",
                    vec![example("Created", 201), example("Bad", 400)],
                ),
            ),
        ])
    }

    fn name(answer: &Answer<'_>) -> String {
        match answer {
            Answer::Example(route) => route.example.name.clone(),
            Answer::NoSuchExample { name, .. } => format!("no {name}"),
            Answer::NotFound => "not found".into(),
        }
    }

    #[test]
    fn paths_reduce_to_what_the_mock_matches() {
        assert_eq!(split_url("{{baseUrl}}/users/:id"), ("/users/:id", None));
        assert_eq!(
            split_url("https://x.test/a/b/?q=1#top"),
            ("/a/b", Some("q=1"))
        );
        assert_eq!(split_url("{{scheme}}://{{host}}/a"), ("/a", None));
        assert_eq!(split_url("{{host}}:8080/a"), ("/a", None));
        assert_eq!(split_url("example.com/a"), ("/a", None));
        assert_eq!(split_url("/a"), ("/a", None));
        assert_eq!(split_url("{{baseUrl}}"), ("", None));
    }

    #[test]
    fn parameters_and_variables_match_any_segment() {
        let routes = routes();
        assert_eq!(name(&routes.find("GET", "/users/7", None, None)), "Found");
        assert_eq!(name(&routes.find("get", "/users/7/", None, None)), "Found");
        assert_eq!(
            name(&routes.find("POST", "/orders/acme/items", None, None)),
            "Created"
        );
        assert_eq!(
            name(&routes.find("DELETE", "/users/7", None, None)),
            "not found"
        );
        assert_eq!(
            name(&routes.find("GET", "/users/7/posts", None, None)),
            "not found"
        );
    }

    #[test]
    fn literal_segments_beat_parameters() {
        assert_eq!(name(&routes().find("GET", "/users/me", None, None)), "Me");
        assert_eq!(name(&routes().find("GET", "/users/m%65", None, None)), "Me");
    }

    #[test]
    fn the_header_picks_an_example_by_name() {
        let routes = routes();
        assert_eq!(
            name(&routes.find("GET", "/users/1", None, Some("Missing"))),
            "Missing"
        );
        assert_eq!(
            name(&routes.find("GET", "/users/1", None, Some("missing"))),
            "Missing"
        );
        let Answer::NoSuchExample { available, .. } =
            routes.find("GET", "/users/1", None, Some("Gone"))
        else {
            panic!()
        };
        assert_eq!(available, ["Missing", "Found"]);
    }

    #[test]
    fn the_query_counts_only_when_an_example_sets_it() {
        let routes = routes();
        assert_eq!(name(&routes.find("GET", "/users", None, None)), "All");
        assert_eq!(
            name(&routes.find("GET", "/users", Some("q=ada&role=admin"), None)),
            "Admins"
        );
        assert_eq!(
            name(&routes.find("GET", "/users", Some("role=guest&q=ada"), None)),
            "All"
        );
    }

    #[test]
    fn head_falls_back_to_get_and_2xx_comes_first() {
        let routes = routes();
        assert_eq!(name(&routes.find("HEAD", "/users/2", None, None)), "Found");
        let only_errors = Routes::from_requests([(
            "x.yaml".to_owned(),
            request(
                "GET",
                "/x",
                vec![example("Later", 503), example("Sooner", 401)],
            ),
        )]);
        assert_eq!(name(&only_errors.find("GET", "/x", None, None)), "Sooner");
    }

    #[test]
    fn the_summary_groups_examples() {
        let summary = routes().summary();
        assert_eq!(summary[0].path, "/users/:id");
        assert_eq!(summary[0].examples.len(), 2);
        assert_eq!(summary[2].path, "/users");
        assert_eq!(summary[3].method, "POST");
    }

    #[test]
    fn examples_load_from_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Shop").unwrap();
        let users = ws.create_folder("", "Users").unwrap();
        ws.create_request(
            &users,
            "Get user",
            &request("GET", "{{baseUrl}}/users/:id", vec![example("Found", 200)]),
        )
        .unwrap();
        ws.create_request(
            "",
            "Health",
            &request("GET", "{{baseUrl}}/health", vec![example("Up", 200)]),
        )
        .unwrap();
        assert_eq!(Routes::load(&ws, "").unwrap().routes().len(), 2);
        let users_only = Routes::load(&ws, &users).unwrap();
        assert_eq!(users_only.routes().len(), 1);
        assert!(matches!(
            users_only.find("GET", "/health", None, None),
            Answer::NotFound
        ));
        assert!(Routes::load(&ws, "nope").is_err());
    }
}
