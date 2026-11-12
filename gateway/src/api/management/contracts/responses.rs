use super::*;

impl Operation {
    pub(super) fn api_response_schema(&self) -> Value {
        let record = |keys: &[&str]| {
            object(
                &keys.iter().map(|k| (*k, string())).collect::<Vec<_>>(),
                keys,
            )
        };
        let array = |items: Value| json!({"type":"array","items":items});
        match self.name.as_str() {
            "hbots_create_bot" | "hbots_get_bot" | "hbots_save_bot" => record(&["slug","name"]),
            "hbots_list_bots" => object(&[("bots",array(record(&["slug","name"]))),("now",json!({"type":"integer"}))],&["bots"]),
            "hbots_rooms_create" | "hbots_room_get" | "hbots_room_update" => record(&["id","title"]),
            "hbots_rooms_list" => object(&[("rooms",array(record(&["id","title"])))],&["rooms"]),
            "hbots_vault_create" | "hbots_vault_get" | "hbots_vault_update" => record(&["id","type"]),
            "hbots_vault_list" => object(&[("items",array(record(&["id","type"])))],&["items"]),
            "hbots_create" => record(&["id"]),
            "hbots_list" => object(&[("attachments",array(record(&["id"])))],&["attachments"]),
            "hbots_download" | "hbots_export" | "hbots_library_file" | "hbots_get_icon" => record(&["contentType","bodyBase64"]),
            "hbots_memory_get" | "hbots_memory_put" => record(&["text","revision"]),
            "hbots_read_activity" => object(&[("input", string()), ("output", string()), ("truncated", boolean())], &["input", "output", "truncated"]),
            "hbots_import" | "hbots_import_csv" => object(&[("added",json!({"type":"integer"})),("updated",json!({"type":"integer"})),("skipped",json!({"type":"integer"}))],&["added","updated","skipped"]),
            _ => dynamic("Original gateway response with domain and provider extension fields. Preserved without transport-specific projection."),
        }
    }
}
