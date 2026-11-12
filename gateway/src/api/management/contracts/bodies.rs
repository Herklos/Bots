use super::*;

fn bot() -> Value {
    let mut schema = string_object(&[
        "name",
        "label",
        "prompt",
        "color",
        "shape",
        "section",
        "parentSlug",
        "brief",
        "firstMessage",
        "createdBy",
        "role",
        "computerId",
    ]);
    for key in [
        "pinned",
        "primary",
        "hidden",
        "hideFromSidebar",
        "inheritParentPrompt",
        "notifications",
    ] {
        schema["properties"][key] = boolean();
    }
    schema["properties"]["subagent"] = json!({"anyOf":[object(&[("prompt",string()),("inheritParentPrompt",boolean())],&[]),{"type":"null"}]});
    schema["properties"]["parentSlug"] = json!({"type":["string","null"]});
    schema["properties"]["code"] = dynamic("Code-mode configuration accepted by the Bot API.");
    schema["properties"]["runtimeMode"] =
        enumeration(&["approval-required", "auto", "full-access", "independent"]);
    schema["properties"]["model"] =
        dynamic("Existing model selection; follows the selected computer's available providers.");
    schema
}
fn room() -> Value {
    let mut schema = string_object(&["title", "description", "color", "shape", "chief", "kickoff"]);
    schema["properties"]["members"] = strings();
    schema["properties"]["kickoff"] = json!({"type":["string","null"]});
    schema["properties"]["notifications"] = boolean();
    schema["properties"]["answerPolicy"] = enumeration(&["default", "last", "everyone", "chief"]);
    schema
}
fn vault() -> Value {
    let mut schema = string_object(&[
        "title", "website", "username", "password", "hints", "name", "label", "street", "apt",
        "city", "state", "zip", "country", "email", "phone", "kind",
    ]);
    schema["properties"]["type"] = enumeration(&["login", "contact"]);
    schema["properties"]["botCanRead"] = boolean();
    for key in ["websites", "emails", "phones"] {
        schema["properties"][key] = strings();
    }
    schema
}
fn chat() -> Value {
    let mut schema = string_object(&["text", "child", "messageId", "replyTo", "quote"]);
    schema["properties"]["attachmentIds"] = strings();
    schema
}
fn settings() -> Value {
    let mut schema = string_object(&[
        "userName",
        "theme",
        "palette",
        "accent",
        "timezone",
        "groupAnswerPolicy",
        "defaultRuntimeMode",
        "maxRuntimeMode",
    ]);
    schema["properties"]["autoReview"] = boolean();
    for key in [
        "rules",
        "beta",
        "notifications",
        "defaultModel",
        "avatarDrawingModel",
        "sections",
    ] {
        schema["properties"][key] =
            dynamic("Existing settings value; validated by the settings API.");
    }
    schema["properties"]["managementMcp"] =
        object(&[("enabled", json!({"const":false}))], &["enabled"]);
    schema
}

impl Operation {
    pub(super) fn body_schema(&self) -> Option<Value> {
        let schema = match self.name.as_str() {
            "hbots_create_bot" | "hbots_save_bot" => bot(),
            "hbots_rooms_create" | "hbots_room_update" => room(),
            "hbots_vault_create" => {
                let mut schema = vault();
                schema["required"] = json!(["type"]);
                schema
            }
            "hbots_vault_update" => vault(),
            "hbots_send_chat" => chat(),
            "hbots_room_send" => string_object(&["text", "messageId"]),
            "hbots_send_voice" => {
                let mut schema =
                    string_object(&["child", "attachmentId", "data", "mediaType", "name"]);
                schema["properties"]["seconds"] = json!({"type":"number"});
                schema["required"] = json!(["seconds"]);
                schema
            }
            "hbots_create" => object(
                &[
                    ("name", string()),
                    ("mediaType", string()),
                    ("data", string()),
                ],
                &["name", "mediaType", "data"],
            ),
            "hbots_respond_request" => object(
                &[
                    ("child", string()),
                    (
                        "kind",
                        enumeration(&[
                            "secret",
                            "secret_request",
                            "approval",
                            "approval_request",
                            "question",
                            "user_input",
                            "user_input_request",
                        ]),
                    ),
                    ("decision", string()),
                    ("secret", string()),
                    ("feedback", string()),
                    ("answer", dynamic("Secret request response.")),
                    (
                        "answers",
                        dynamic("Harness question answers keyed by question ID."),
                    ),
                ],
                &["kind"],
            ),
            "hbots_library_save" => {
                string_object(&["id", "kind", "title", "status", "body", "url", "path"])
            }
            "hbots_connector_save" => object(
                &[
                    ("name", string()),
                    ("url", string()),
                    ("secret", string()),
                    ("enabled", boolean()),
                ],
                &["name", "url"],
            ),
            "hbots_connector_delete" => object(&[("name", string())], &["name"]),
            "hbots_model_put" => {
                let mut schema = string_object(&["instanceId", "model", "effort", "runtimeMode"]);
                schema["properties"]["confirmSwitch"] = boolean();
                schema
            }
            "hbots_bot_routine_save" => object(
                &[
                    ("title", string()),
                    ("kind", enumeration(&["hour", "day", "once", "interval"])),
                ],
                &[],
            ),
            "hbots_bot_routine_enabled" | "hbots_routines_enabled" => {
                object(&[("id", string()), ("enabled", boolean())], &["id"])
            }
            "hbots_bot_routine_delete"
            | "hbots_bot_routine_run"
            | "hbots_routines_delete"
            | "hbots_routines_run" => object(&[("id", string())], &["id"]),
            "hbots_routines_upsert" => object(
                &[
                    ("id", string()),
                    ("title", string()),
                    ("prompt", string()),
                    ("threadId", string()),
                    ("enabled", boolean()),
                    ("schedule", dynamic("Harness schedule configuration.")),
                    (
                        "modelSelection",
                        dynamic("Harness provider/model selection."),
                    ),
                    ("runtimeMode", string()),
                ],
                &[],
            ),
            "hbots_memory_put" => object(
                &[("text", string()), ("revision", string())],
                &["text", "revision"],
            ),
            "hbots_computer_open" | "hbots_computer_close" => object(
                &[
                    ("width", json!({"type":"integer"})),
                    ("height", json!({"type":"integer"})),
                ],
                &[],
            ),
            "hbots_computer_control" => object(
                &[
                    ("tabId", string()),
                    ("action", string()),
                    ("url", string()),
                    ("width", json!({"type":"integer"})),
                    ("height", json!({"type":"integer"})),
                ],
                &[],
            ),
            "hbots_computer_input" => object(
                &[
                    ("kind", string()),
                    ("text", string()),
                    ("key", string()),
                    ("keyCode", json!({"type":"integer"})),
                    (
                        "modifiers",
                        dynamic("Modifier flags accepted by the computer API."),
                    ),
                    ("x", json!({"type":"number"})),
                    ("y", json!({"type":"number"})),
                    ("deltaX", json!({"type":"number"})),
                    ("deltaY", json!({"type":"number"})),
                ],
                &["kind"],
            ),
            "hbots_main_bot" => object(&[("slug", string())], &["slug"]),
            "hbots_import" => object(
                &[("items", json!({"type":"array","items":vault()}))],
                &["items"],
            ),
            "hbots_import_csv" => {
                json!({"type":"string","description":"UTF-8 CSV contents, sent as text/csv."})
            }
            "hbots_access_password" => object(
                &[("password", json!({"type":["string","null"]}))],
                &["password"],
            ),
            "hbots_access_network" => object(
                &[("mode", enumeration(&["local", "lan", "tailscale"]))],
                &["mode"],
            ),
            "hbots_access_tailscale" | "hbots_access_tunnel" => object(&[("on", boolean())], &[]),
            "hbots_put_storage" => object(
                &[
                    ("backend", enumeration(&["filesystem", "anytype"])),
                    ("root", json!({"type":["string","null"]})),
                    ("host", string()),
                    ("space", string()),
                ],
                &[],
            ),
            "hbots_test_storage" | "hbots_copy_storage" => string_object(&["root"]),
            "hbots_put_storage_key" => object(&[("key", string())], &["key"]),
            "hbots_pair_start" => string_object(&["host", "appName"]),
            "hbots_pair_confirm" => object(
                &[("challengeId", string()), ("code", string())],
                &["challengeId", "code"],
            ),
            "hbots_anytype_setup" => object(&[("dryRun", boolean())], &[]),
            "hbots_thread_previews" => object(
                &[("stamps", dynamic("Thread-ID to preview-stamp mapping."))],
                &[],
            ),
            "hbots_thread_reaction" => object(
                &[("item", string()), ("emoji", string())],
                &["item", "emoji"],
            ),
            "hbots_thread_pin" => object(&[("item", string())], &["item"]),
            "hbots_thread_reply" => string_object(&["messageId", "replyTo", "text"]),
            "hbots_thread_read" => object(
                &[
                    ("latest", string()),
                    ("open", boolean()),
                    ("hidden", boolean()),
                ],
                &[],
            ),
            "hbots_thread_voice" => object(
                &[("at", string()), ("seconds", json!({"type":"number"}))],
                &[],
            ),
            "hbots_put_settings" => settings(),
            "hbots_set_auto_update" => object(&[("auto", boolean())], &["auto"]),
            "hbots_stop_chat" => string_object(&["child"]),
            "hbots_open_side" => string_object(&["text"]),
            "hbots_merge_side" => object(&[("threadId", string())], &[]),
            "hbots_environment_put" => object(&[("id", string())], &["id"]),
            "hbots_mark_seen"
            | "hbots_room_seen"
            | "hbots_access_network_restart"
            | "hbots_restart_storage"
            | "hbots_anytype_check"
            | "hbots_start_update" => object(&[], &[]),
            "hbots_delete_bot"
            | "hbots_remove"
            | "hbots_library_delete"
            | "hbots_vault_delete"
            | "hbots_delete_storage_key"
            | "hbots_room_delete"
            | "hbots_dismiss_notice" => return None,
            _ if self.method == "get" => return None,
            _ => panic!("Missing request contract for {}", self.name),
        };
        Some(schema)
    }
}
