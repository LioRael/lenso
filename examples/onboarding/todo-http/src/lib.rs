//! A single Plugin owns temporary Todo state and its HTTP API.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

use lenso_capability_http_endpoint::{
    HandleResponse,
    prelude::*,
    response::{self, Problem, StatusCode},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTodo {
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceTodo {
    title: String,
    completed: bool,
}

#[derive(Debug, Deserialize)]
struct TodoPath {
    todo_id: String,
}

#[derive(Clone, Debug, Serialize)]
struct Todo {
    id: String,
    title: String,
    completed: bool,
}

/// One Instance owns one store; dispatch clones share that store.
#[lenso::plugin]
#[derive(Clone, Debug, Default)]
pub struct TodoHttp {
    next_id: Rc<Cell<u64>>,
    todos: Rc<RefCell<BTreeMap<String, Todo>>>,
}

#[endpoint]
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
impl TodoHttp {
    #[post("todos.create", "/todos")]
    async fn create(
        &self,
        Json(input): Json<CreateTodo>,
    ) -> Result<(StatusCode, Json<Todo>), Problem> {
        let title = validated_title(&input.title)?;
        let sequence = self.next_id.get() + 1;
        self.next_id.set(sequence);
        let todo = Todo {
            id: format!("todo-{sequence}"),
            title,
            completed: false,
        };
        self.todos
            .borrow_mut()
            .insert(todo.id.clone(), todo.clone());
        Ok((StatusCode::CREATED, Json(todo)))
    }

    #[get("todos.list", "/todos")]
    async fn list(&self) -> Result<Json<Vec<Todo>>, Problem> {
        Ok(Json(self.todos.borrow().values().cloned().collect()))
    }

    #[get("todos.read", "/todos/{todo_id}")]
    async fn read(&self, Path(path): Path<TodoPath>) -> Result<Json<Todo>, Problem> {
        self.todos
            .borrow()
            .get(&path.todo_id)
            .cloned()
            .map(Json)
            .ok_or_else(todo_not_found)
    }

    #[put("todos.replace", "/todos/{todo_id}")]
    async fn replace(
        &self,
        Path(path): Path<TodoPath>,
        Json(input): Json<ReplaceTodo>,
    ) -> Result<Json<Todo>, Problem> {
        let title = validated_title(&input.title)?;
        let mut todos = self.todos.borrow_mut();
        let todo = todos.get_mut(&path.todo_id).ok_or_else(todo_not_found)?;
        todo.title = title;
        todo.completed = input.completed;
        Ok(Json(todo.clone()))
    }

    #[delete("todos.delete", "/todos/{todo_id}")]
    async fn delete(&self, Path(path): Path<TodoPath>) -> Result<HandleResponse, Problem> {
        self.todos
            .borrow_mut()
            .remove(&path.todo_id)
            .ok_or_else(todo_not_found)?;
        Ok(response::empty(StatusCode::NO_CONTENT))
    }
}

fn validated_title(title: &str) -> Result<String, Problem> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(Problem::new(
            StatusCode::BAD_REQUEST,
            "invalid_title",
            "title must contain between 1 and 200 characters after trimming",
        ));
    }
    Ok(title.to_owned())
}

fn todo_not_found() -> Problem {
    Problem::new(
        StatusCode::NOT_FOUND,
        "todo_not_found",
        "the todo does not exist",
    )
}
