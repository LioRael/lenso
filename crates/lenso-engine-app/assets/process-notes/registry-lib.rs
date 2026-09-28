use std::{cell::{Cell, RefCell}, collections::BTreeMap};

use lenso_capability_http_endpoint::{
    Bytes, CAPABILITY_ID, DESCRIBE_OPERATION, DESCRIPTOR_VERSION, HANDLE_OPERATION,
    DescribeResponse, DescribeResponseRoutesItem, HandleRequest, HandleResponse,
    HandleResponseHeadersItem,
};
use lenso_process_sdk::{ProcessOutcome, ProcessPlugin};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateNote { title: String, body: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Note { id: String, title: String, body: String }

#[derive(Debug, Default)]
pub struct Notes {
    next_id: Cell<u64>,
    notes: RefCell<BTreeMap<String, Note>>,
}

impl Notes {
    fn describe() -> ProcessOutcome {
        let routes = [
            ("notes.create", "POST", "/notes"),
            ("notes.read", "GET", "/notes/{id}"),
        ].into_iter().map(|(route_id, method, path)| DescribeResponseRoutesItem {
            route_id: route_id.into(), method: method.into(), path: path.into(), openapi: None,
        }).collect();
        Self::success(DescribeResponse { routes })
    }

    fn success(value: impl Serialize) -> ProcessOutcome {
        match serde_json::to_value(value) {
            Ok(value) => ProcessOutcome::Success(value),
            Err(error) => ProcessOutcome::Failure(error.to_string()),
        }
    }

    fn response(status: i64, value: impl Serialize) -> ProcessOutcome {
        let body = match serde_json::to_vec(&value) {
            Ok(body) => body,
            Err(error) => return ProcessOutcome::Failure(error.to_string()),
        };
        Self::success(HandleResponse {
            status,
            headers: vec![HandleResponseHeadersItem {
                name: "content-type".into(), value: "application/json; charset=utf-8".into(),
            }],
            body: Bytes::from(body),
        })
    }

    fn handle(&self, request: Value) -> ProcessOutcome {
        let request: HandleRequest = match serde_json::from_value(request) {
            Ok(request) => request,
            Err(error) => return ProcessOutcome::Failure(format!("invalid Endpoint request: {error}")),
        };
        match request.route_id.as_str() {
            "notes.create" if request.method == "POST" => {
                let input: CreateNote = match serde_json::from_slice(request.body.as_ref()) {
                    Ok(input) => input,
                    Err(_) => return Self::response(400, json!({"error":"invalid JSON note"})),
                };
                if input.title.trim().is_empty() {
                    return Self::response(400, json!({"error":"title is required"}));
                }
                let id = (self.next_id.get() + 1).to_string();
                self.next_id.set(self.next_id.get() + 1);
                let note = Note { id: id.clone(), title: input.title, body: input.body };
                self.notes.borrow_mut().insert(id, note.clone());
                Self::response(201, note)
            }
            "notes.read" if request.method == "GET" => {
                let id = request.path_parameters.iter()
                    .find(|parameter| parameter.name == "id")
                    .map(|parameter| parameter.value.as_str());
                match id.and_then(|id| self.notes.borrow().get(id).cloned()) {
                    Some(note) => Self::response(200, note),
                    None => Self::response(404, json!({"error":"note not found"})),
                }
            }
            _ => ProcessOutcome::Failure("unknown notes route".into()),
        }
    }
}

impl ProcessPlugin for Notes {
    fn descriptor(&self) -> Value {
        json!({
            "abi": "lenso.json-request@1",
            "capabilities": [{
                "capability_id": CAPABILITY_ID,
                "descriptor_version": DESCRIPTOR_VERSION,
                "request_operations": [DESCRIBE_OPERATION, HANDLE_OPERATION],
            }],
        })
    }

    fn invoke(&self, capability: &str, operation: &str, request: Value) -> ProcessOutcome {
        if capability != CAPABILITY_ID {
            return ProcessOutcome::Failure("unknown Capability".into());
        }
        match operation {
            DESCRIBE_OPERATION => Self::describe(),
            HANDLE_OPERATION => self.handle(request),
            _ => ProcessOutcome::Failure("unknown Endpoint operation".into()),
        }
    }
}

pub fn serve() {
    lenso_process_sdk::serve(&Notes::default()).expect("serve trusted Process Plugin");
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_capability_http_endpoint::{HandleRequestPathParametersItem, DescribeResponse};

    #[test]
    fn notes_create_then_read_via_endpoint_contract() {
        let notes = Notes::default();
        let ProcessOutcome::Success(described) = notes.invoke(CAPABILITY_ID, DESCRIBE_OPERATION, json!({})) else { panic!("describe failed") };
        let routes: DescribeResponse = serde_json::from_value(described).unwrap();
        assert_eq!(routes.routes.len(), 2);

        let request = |route_id: &str, method: &str, body: Vec<u8>, path_parameters: Vec<HandleRequestPathParametersItem>| HandleRequest {
            route_id: route_id.into(), method: method.into(), body: Bytes::from(body),
            path: "/notes".into(), path_parameters, headers: vec![], credential: None,
            query: None, request_id: "test".into(),
        };
        let ProcessOutcome::Success(created) = notes.invoke(CAPABILITY_ID, HANDLE_OPERATION,
            serde_json::to_value(request("notes.create", "POST", br#"{"title":"First","body":"Hello"}"#.to_vec(), vec![])).unwrap()) else { panic!("create failed") };
        let created: HandleResponse = serde_json::from_value(created).unwrap();
        assert_eq!(created.status, 201);
        let note: Note = serde_json::from_slice(created.body.as_ref()).unwrap();
        let ProcessOutcome::Success(found) = notes.invoke(CAPABILITY_ID, HANDLE_OPERATION,
            serde_json::to_value(request("notes.read", "GET", vec![], vec![HandleRequestPathParametersItem {
                name: "id".into(), value: note.id.clone(),
            }])).unwrap()) else { panic!("read failed") };
        let found: HandleResponse = serde_json::from_value(found).unwrap();
        assert_eq!(found.status, 200);
        let read: Note = serde_json::from_slice(found.body.as_ref()).unwrap();
        assert_eq!(read.title, "First");
    }
}
