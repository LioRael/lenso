use futures::executor::block_on;
use lenso_capability_http_endpoint::{response::StatusCode, testing::EndpointTest};
use local_starter::Notes;
use serde_json::{Value, json};

#[test]
fn create_then_read_a_note() {
    block_on(async {
        let app = EndpointTest::new(Notes::default());
        let created = app
            .request("notes.create")
            .json(&json!({"title": "First", "body": "Hello"}))
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let note: Value = created.json().unwrap();
        let found = app
            .request("notes.read")
            .path_parameter("id", note["id"].as_str().unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(found.status(), StatusCode::OK);
        assert_eq!(found.json::<Value>().unwrap(), note);
    });
}

#[test]
fn invalid_and_missing_notes_are_http_problems() {
    block_on(async {
        let app = EndpointTest::new(Notes::default());
        let invalid = app
            .request("notes.create")
            .json(&json!({"title": " ", "body": "Hello"}))
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_eq!(invalid.json::<Value>().unwrap()["code"], "invalid_title");

        let missing = app
            .request("notes.read")
            .path_parameter("id", "missing")
            .send()
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_eq!(missing.json::<Value>().unwrap()["code"], "note_not_found");
    });
}
