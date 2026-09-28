use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

use lenso_capability_http_endpoint::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateNote {
    title: String,
    body: String,
}

#[derive(Clone, Debug, Serialize)]
struct Note {
    id: String,
    title: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct NotePath {
    id: String,
}

#[derive(Clone, Debug, Default)]
pub struct Notes {
    next_id: Rc<Cell<u64>>,
    notes: Rc<RefCell<BTreeMap<String, Note>>>,
}

#[endpoint(standalone)]
impl Notes {
    #[post("notes.create", "/notes")]
    async fn create(
        &self,
        Json(input): Json<CreateNote>,
    ) -> Result<(StatusCode, Json<Note>), Problem> {
        if input.title.trim().is_empty() {
            return Err(Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid_title",
                "title is required",
            ));
        }
        let next_id = self.next_id.get().checked_add(1).ok_or_else(|| {
            Problem::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "note_ids_exhausted",
                "no more note identifiers are available",
            )
        })?;
        self.next_id.set(next_id);
        let note = Note {
            id: next_id.to_string(),
            title: input.title,
            body: input.body,
        };
        self.notes
            .borrow_mut()
            .insert(note.id.clone(), note.clone());
        Ok((StatusCode::CREATED, Json(note)))
    }

    #[get("notes.read", "/notes/{id}")]
    async fn read(&self, Path(path): Path<NotePath>) -> Result<Json<Note>, Problem> {
        self.notes
            .borrow()
            .get(&path.id)
            .cloned()
            .map(Json)
            .ok_or_else(|| Problem::new(StatusCode::NOT_FOUND, "note_not_found", "note not found"))
    }
}
