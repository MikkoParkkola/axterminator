use std::collections::HashSet;

use crate::mcp::protocol::Tool;

pub(crate) fn assert_tool_names_unique(tools: &[Tool], scope: &str) {
    let names: HashSet<&str> = tools.iter().map(|tool| tool.name).collect();
    assert_eq!(names.len(), tools.len(), "duplicate tool names in {scope}");
}
