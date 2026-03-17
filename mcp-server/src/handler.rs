use std::path::PathBuf;

use savhub_local::presets::{ResolvedSkill, resolve_skills_for_project};
use savhub_local::skills::{extract_skill_description, read_skill_md};
use serde_json::{Value, json};

use crate::protocol::*;

pub struct McpHandler {
    workdir: PathBuf,
}

impl McpHandler {
    pub fn new(workdir: PathBuf, _preset_override: Option<String>) -> Self {
        Self { workdir }
    }

    pub async fn handle_request(&self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = request.id.clone();
        let params = request.params.clone().unwrap_or(Value::Null);

        match request.method.as_str() {
            METHOD_INITIALIZE => Some(self.handle_initialize(id)),
            METHOD_INITIALIZED => {
                // Notification, no response needed
                None
            }
            METHOD_PING => Some(JsonRpcResponse::success(id, json!({}))),
            METHOD_PROMPTS_LIST => Some(self.handle_prompts_list(id)),
            METHOD_PROMPTS_GET => Some(self.handle_prompts_get(id, &params)),
            METHOD_RESOURCES_LIST => Some(self.handle_resources_list(id)),
            METHOD_RESOURCES_READ => Some(self.handle_resources_read(id, &params)),
            METHOD_TOOLS_LIST => Some(self.handle_tools_list(id)),
            METHOD_TOOLS_CALL => Some(self.handle_tools_call(id, &params)),
            _ => {
                eprintln!("savhub-mcp: unknown method: {}", request.method);
                if id.is_some() {
                    Some(JsonRpcResponse::method_not_found(id, &request.method))
                } else {
                    None
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // initialize
    // -----------------------------------------------------------------------

    fn handle_initialize(&self, id: Option<Value>) -> JsonRpcResponse {
        eprintln!(
            "savhub-mcp: initializing for workdir={}",
            self.workdir.display()
        );
        JsonRpcResponse::success(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "prompts": { "listChanged": true },
                    "resources": {},
                    "tools": {}
                },
                "serverInfo": {
                    "name": "savhub",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
    }

    // -----------------------------------------------------------------------
    // prompts
    // -----------------------------------------------------------------------

    fn handle_prompts_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let skills = self.resolve_skills();
        let prompts: Vec<Value> = skills
            .iter()
            .map(|skill| {
                let description = read_skill_md(&skill.folder)
                    .map(|c| extract_skill_description(&c))
                    .unwrap_or_default();
                json!({
                    "name": skill.slug,
                    "description": if description.is_empty() {
                        skill.display_name.clone()
                    } else {
                        description
                    },
                    "arguments": []
                })
            })
            .collect();

        JsonRpcResponse::success(id, json!({ "prompts": prompts }))
    }

    fn handle_prompts_get(&self, id: Option<Value>, params: &Value) -> JsonRpcResponse {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return JsonRpcResponse::error(id, -32602, "Missing required parameter: name");
        };

        let skills = self.resolve_skills();
        let Some(skill) = skills.iter().find(|s| s.slug == name) else {
            return JsonRpcResponse::error(id, -32602, format!("Prompt not found: {name}"));
        };

        let content = read_skill_md(&skill.folder)
            .unwrap_or_else(|| format!("# {}\n\nSkill content not available.", skill.display_name));

        let description = extract_skill_description(&content);

        JsonRpcResponse::success(
            id,
            json!({
                "description": if description.is_empty() {
                    &skill.display_name
                } else {
                    &description
                },
                "messages": [
                    {
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": content
                        }
                    }
                ]
            }),
        )
    }

    // -----------------------------------------------------------------------
    // resources (fallback for clients without prompts support)
    // -----------------------------------------------------------------------

    fn handle_resources_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let skills = self.resolve_skills();
        let resources: Vec<Value> = skills
            .iter()
            .map(|skill| {
                let description = read_skill_md(&skill.folder)
                    .map(|c| extract_skill_description(&c))
                    .unwrap_or_default();
                json!({
                    "uri": format!("skill://{}", skill.slug),
                    "name": skill.display_name,
                    "description": if description.is_empty() {
                        None
                    } else {
                        Some(description)
                    },
                    "mimeType": "text/markdown"
                })
            })
            .collect();

        JsonRpcResponse::success(id, json!({ "resources": resources }))
    }

    fn handle_resources_read(&self, id: Option<Value>, params: &Value) -> JsonRpcResponse {
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return JsonRpcResponse::error(id, -32602, "Missing required parameter: uri");
        };

        let slug = uri.strip_prefix("skill://").unwrap_or(uri);
        let skills = self.resolve_skills();
        let Some(skill) = skills.iter().find(|s| s.slug == slug) else {
            return JsonRpcResponse::error(id, -32602, format!("Resource not found: {uri}"));
        };

        let content = read_skill_md(&skill.folder)
            .unwrap_or_else(|| "Skill content not available.".to_string());

        JsonRpcResponse::success(
            id,
            json!({
                "contents": [
                    {
                        "uri": uri,
                        "mimeType": "text/markdown",
                        "text": content
                    }
                ]
            }),
        )
    }

    // -----------------------------------------------------------------------
    // tools (management operations)
    // -----------------------------------------------------------------------

    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(
            id,
            json!({
                "tools": [
                    {
                        "name": "search_skills",
                        "description": "Search the savhub registry for available skills by keyword",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {
                                    "type": "string",
                                    "description": "Search query (keyword, slug, or category)"
                                }
                            },
                            "required": ["query"]
                        }
                    },
                    {
                        "name": "install_skill",
                        "description": "Install a skill from the savhub registry by slug. After installation the skill becomes available as a prompt immediately.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "slug": {
                                    "type": "string",
                                    "description": "Skill slug to install"
                                }
                            },
                            "required": ["slug"]
                        }
                    }
                ]
            }),
        )
    }

    fn handle_tools_call(&self, id: Option<Value>, params: &Value) -> JsonRpcResponse {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return JsonRpcResponse::error(id, -32602, "Missing required parameter: name");
        };
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        match name {
            "search_skills" => self.tool_search_skills(id, &arguments),
            "install_skill" => self.tool_install_skill(id, &arguments),
            _ => JsonRpcResponse::error(id, -32602, format!("Unknown tool: {name}")),
        }
    }

    fn tool_search_skills(&self, id: Option<Value>, arguments: &Value) -> JsonRpcResponse {
        let Some(query) = arguments.get("query").and_then(Value::as_str) else {
            return JsonRpcResponse::success(id, tool_error("Missing parameter: query"));
        };

        match savhub_local::registry::search_skill_entries(query, 20) {
            Ok(entries) => {
                let results: Vec<Value> = entries
                    .iter()
                    .map(|e| {
                        json!({
                            "slug": e.slug,
                            "name": e.name,
                            "description": e.description,
                            "version": e.version,
                            "categories": e.categories,
                        })
                    })
                    .collect();

                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&results).unwrap_or_default()
                        }]
                    }),
                )
            }
            Err(e) => JsonRpcResponse::success(id, tool_error(&format!("Search failed: {e}"))),
        }
    }

    fn tool_install_skill(&self, id: Option<Value>, arguments: &Value) -> JsonRpcResponse {
        let Some(sign) = arguments.get("sign").and_then(Value::as_str) else {
            return JsonRpcResponse::success(id, tool_error("Missing parameter: sign"));
        };

        match savhub_local::registry::install_skill_from_registry(&sign) {
            Ok(path) => {
                // Notify client that prompts list has changed
                let _ = crate::transport::send_notification(
                    "notifications/prompts/list_changed",
                    json!({}),
                );

                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": format!(
                                "Installed skill '{sign}' to {}. The skill is now available as a prompt.",
                                path.display()
                            )
                        }]
                    }),
                )
            }
            Err(e) => JsonRpcResponse::success(
                id,
                tool_error(&format!("Failed to install skill '{sign}': {e}")),
            ),
        }
    }

    // -----------------------------------------------------------------------
    // helpers
    // -----------------------------------------------------------------------

    fn resolve_skills(&self) -> Vec<ResolvedSkill> {
        resolve_skills_for_project(&self.workdir).unwrap_or_default()
    }
}

fn tool_error(message: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": message
        }],
        "isError": true
    })
}
