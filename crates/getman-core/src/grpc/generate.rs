//! Writing one request per method of a service, to start from.

use super::ServiceInfo;
use crate::error::Result;
use crate::model::{Grpc, Request, RequestKind};
use crate::workspace::Workspace;

/// Creates a folder named after `service` inside `folder`, with one
/// request per method holding an example message. The requests take their
/// address, schema and TLS setting from `template`; its headers and
/// authentication go on the new folder, so the requests inherit them.
/// Returns the new folder's path and the number of requests.
pub fn generate_requests(
    workspace: &Workspace,
    folder: &str,
    template: &Request,
    service: &ServiceInfo,
) -> Result<(String, usize)> {
    let name = service
        .name
        .rsplit('.')
        .next()
        .unwrap_or(&service.name)
        .to_owned();
    let path = workspace.create_folder(folder, &name)?;
    let mut meta = workspace.read_folder(&path)?;
    meta.description = format!("A request for each method of {}.", service.name);
    meta.headers.clone_from(&template.headers);
    meta.auth = template.auth.clone();
    workspace.write_folder(&path, &meta)?;
    for method in &service.methods {
        let message = serde_json::to_string_pretty(&method.example).unwrap_or_default();
        let request = Request {
            kind: RequestKind::Grpc,
            method: method.path.clone(),
            url: template.url.clone(),
            grpc: Grpc {
                proto: template.grpc.proto.clone(),
                tls: template.grpc.tls,
                message: format!("{message}\n"),
            },
            settings: template.settings,
            ..Request::default()
        };
        workspace.create_request(&path, &method.name, &request)?;
    }
    Ok((path, service.methods.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::{CallType, MethodInfo};
    use crate::model::{Auth, KeyValue};

    #[test]
    fn every_method_gets_a_request_in_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Shop").unwrap();
        let method = |name: &str| MethodInfo {
            name: name.into(),
            path: format!("shop.Orders/{name}"),
            input: "shop.OrderId".into(),
            output: "shop.Order".into(),
            call_type: CallType::Unary,
            example: serde_json::json!({ "id": "" }),
        };
        let service = ServiceInfo {
            name: "shop.Orders".into(),
            methods: vec![method("Get"), method("Cancel")],
        };
        let template = Request {
            kind: RequestKind::Grpc,
            url: "{{ordersHost}}".into(),
            grpc: Grpc {
                proto: "protos/shop.proto".into(),
                tls: true,
                message: "{}".into(),
            },
            headers: vec![KeyValue::new("x-tenant", "acme")],
            auth: Auth::Bearer {
                token: "{{token}}".into(),
                prefix: String::new(),
            },
            ..Request::default()
        };
        let (folder, count) = generate_requests(&workspace, "", &template, &service).unwrap();
        assert_eq!((folder.as_str(), count), ("orders", 2));
        let meta = workspace.read_folder("orders").unwrap();
        assert_eq!(meta.headers, template.headers);
        assert!(matches!(meta.auth, Auth::Bearer { .. }));
        let get = workspace.read_request("orders/get.yaml").unwrap();
        assert_eq!(get.method, "shop.Orders/Get");
        assert_eq!(get.url, "{{ordersHost}}");
        assert!(get.grpc.tls);
        assert_eq!(get.grpc.message, "{\n  \"id\": \"\"\n}\n");
        assert!(get.auth.is_inherit());
        assert!(get.headers.is_empty());
    }
}
