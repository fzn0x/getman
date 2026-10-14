//! Example requests for a new workspace, against JSONPlaceholder, a free
//! public test API.

use getman_core::model::{Body, Environment, Request, Scripts};
use getman_core::workspace::EnvironmentFiles;
use getman_core::{Result, Vars, Workspace};

/// Adds the example folder, requests and environments to a workspace.
pub(crate) fn add(workspace: &Workspace) -> Result<()> {
    let mut meta = workspace.read_folder("")?;
    meta.variables
        .entry("baseUrl".into())
        .or_insert_with(|| "https://jsonplaceholder.typicode.com".into());
    workspace.write_folder("", &meta)?;

    for (name, base) in [
        ("public", "https://jsonplaceholder.typicode.com"),
        ("local", "http://localhost:3000"),
    ] {
        if workspace.environments()?.iter().any(|env| env.id == name) {
            continue;
        }
        let id = workspace.create_environment(name)?;
        let variables: Vars = [("baseUrl".to_owned(), base.to_owned())]
            .into_iter()
            .collect();
        workspace.write_environment(
            &id,
            &EnvironmentFiles {
                shared: Environment {
                    variables,
                    ..Environment::default()
                },
                local: Vars::new(),
            },
        )?;
    }

    let folder = workspace.create_folder("", "Posts")?;
    let mut posts = workspace.read_folder(&folder)?;
    posts.description = "Example requests. Run them all with `getman run posts`.".into();
    workspace.write_folder(&folder, &posts)?;

    let requests = [
        Request {
            name: "List posts".into(),
            url: "{{baseUrl}}/posts?_limit=5".into(),
            description: "Checks need no code: each line is a target, an operator and a value.".into(),
            checks: vec![
                "status == 200".into(),
                "header.content-type contains json".into(),
                "json.length == 5".into(),
                "json[0].id exists".into(),
            ],
            ..Request::default()
        },
        Request {
            name: "Get a post".into(),
            url: "{{baseUrl}}/posts/:id".into(),
            path_params: [("id".to_owned(), "1".to_owned())].into_iter().collect(),
            scripts: Scripts {
                pre_request: String::new(),
                post_response: [
                    "gm.test(\"returns the post\", () => {",
                    "  gm.response.to.have.status(200);",
                    "  const post = gm.response.json();",
                    "  gm.expect(post).to.have.property(\"id\", 1);",
                    "  gm.expect(post.title).to.be.a(\"string\").that.is.not.empty;",
                    "});",
                ]
                .join("\n"),
            },
            ..Request::default()
        },
        Request {
            name: "Create a post".into(),
            method: "POST".into(),
            url: "{{baseUrl}}/posts".into(),
            body: Body::Json {
                content: "{\n  \"title\": \"{{$randomWords}}\",\n  \"body\": \"{{$randomLoremSentence}}\",\n  \"userId\": 1\n}\n".into(),
            },
            checks: vec!["status == 201".into(), "json.id is number".into()],
            captures: [("newPostId".to_owned(), "json.id".to_owned())].into_iter().collect(),
            ..Request::default()
        },
    ];
    let mut order = Vec::new();
    for request in &requests {
        let path = workspace.create_request(&folder, &request.name, request)?;
        order.push(
            path.rsplit('/')
                .next()
                .unwrap_or_default()
                .trim_end_matches(".yaml")
                .to_owned(),
        );
    }
    workspace.set_order(&folder, order)?;
    Ok(())
}
