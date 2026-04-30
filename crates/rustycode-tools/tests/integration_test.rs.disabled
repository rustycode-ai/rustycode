/// Integration tests for TodoWrite tool and layered prompt features
use rustycode_tools::{
    new_todo_state, TodoUpdateTool, TodoWriteTool, Tool, ToolContext, ToolExecutor,
};
use serde_json::json;

#[test]
fn test_todo_write_tool_registration() {
    let todo_state = new_todo_state();
    let executor = ToolExecutor::with_todo_state(std::env::current_dir().unwrap(), todo_state);

    // Check that TodoWrite and TodoUpdate tools are registered
    let tools = executor.list();
    let tool_names: Vec<_> = tools.iter().map(|t| t.name.clone()).collect();

    assert!(
        tool_names.contains(&"todo_write".to_string()),
        "todo_write should be registered"
    );
    assert!(
        tool_names.contains(&"todo_update".to_string()),
        "todo_update should be registered"
    );
}

#[test]
fn test_todo_write_execution() {
    let todo_state = new_todo_state();
    let ctx = ToolContext::new(std::env::current_dir().unwrap());

    // Create todo_write tool
    let tool = TodoWriteTool::new(todo_state.clone());

    // Execute with sample todos
    let params = json!({
        "title": "Test Task List",
        "todos": [
            {"id": "1", "title": "First task", "status": "pending"},
            {"id": "2", "title": "Second task", "status": "in_progress"},
            {"id": "3", "title": "Third task", "status": "completed"}
        ]
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok(), "todo_write should execute successfully");

    let output = result.unwrap();
    assert!(
        !output.text.is_empty(),
        "todo_write should return output text"
    );

    // Verify todos were stored
    let todos = todo_state.lock().unwrap();
    assert_eq!(todos.len(), 3, "Should have 3 todos");
    assert_eq!(todos[0].title, "First task");
    assert_eq!(todos[1].status, rustycode_tools::TodoStatus::InProgress);
    assert_eq!(todos[2].status, rustycode_tools::TodoStatus::Completed);
}

#[test]
fn test_todo_update_execution() {
    let todo_state = new_todo_state();

    // First, create some todos
    {
        let mut todos = todo_state.lock().unwrap();
        todos.push(rustycode_tools::TodoItem {
            id: "1".to_string(),
            title: "Task to update".to_string(),
            status: rustycode_tools::TodoStatus::Pending,
        });
    }

    let ctx = ToolContext::new(std::env::current_dir().unwrap());
    let tool = TodoUpdateTool::new(todo_state.clone());

    // Update the todo
    let params = json!({
        "id": "1",
        "status": "completed"
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok(), "todo_update should execute successfully");

    // Verify the update
    let todos = todo_state.lock().unwrap();
    assert_eq!(todos[0].status, rustycode_tools::TodoStatus::Completed);
}
