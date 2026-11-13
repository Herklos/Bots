use super::*;

mod assets;
mod guard;
mod infrastructure;
mod proxy;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use assets::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use guard::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use proxy::*;

pub(crate) fn app(state: Arc<AppState>) -> Router {
    let (api, _) = application_routes();
    let api = api
        .merge(infrastructure::control_routes().0)
        .layer(middleware::from_fn_with_state(state.clone(), require_login));
    infrastructure::public_routes()
        .0
        .merge(api)
        .fallback(static_files)
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

#[cfg(test)]
pub(crate) fn excluded_operations() -> Vec<Value> {
    infrastructure::manifest()
}

pub(crate) fn application_routes() -> (
    Router<Arc<AppState>>,
    Vec<crate::api::management::Operation>,
) {
    let mut router = Router::new();
    let mut operations = Vec::new();
    macro_rules! record {
        ($path:literal, layer, $arg:expr) => {};
        ($path:literal, $method:ident, $handler:expr) => {
            operations.push(crate::api::management::Operation::new(
                $path,
                stringify!($method),
                stringify!($handler),
            ));
        };
    }
    macro_rules! route {
        ($path:literal, $first:ident($handler:path) $(.$method:ident($arg:expr))*) => {{
            router = router.route($path, $first($handler) $(.$method($arg))*);
            record!($path, $first, $handler);
            $(record!($path, $method, $arg);)*
        }};
    }
    route!("/api/config", get(config_route));
    route!("/api/settings", get(get_settings).put(put_settings));
    route!("/api/diagnostics", get(get_diagnostics));
    route!("/api/diagnostics/ask", post(ask_diagnostics));
    route!("/api/bots", get(list_bots).post(create_bot));
    route!(
        "/api/bots/{slug}",
        get(get_bot).put(save_bot).delete(delete_bot)
    );
    route!("/api/bots/{slug}/seen", post(mark_seen));
    route!("/api/bots/{slug}/chat", get(read_chat).post(send_chat));
    route!("/api/bots/{slug}/chat/activity/{id}", get(read_activity));
    route!("/api/bots/{slug}/side", get(list_side).post(open_side));
    route!("/api/bots/{slug}/side/merge", post(merge_side));
    route!("/api/bots/{slug}/voice", post(send_voice));
    route!("/api/bots/{slug}/stop", post(stop_chat));
    route!(
        "/api/bots/{slug}/attachments",
        get(crate::chat::attachments::list)
            .post(crate::chat::attachments::create)
            .layer(axum::extract::DefaultBodyLimit::max(12 << 20))
    );
    route!(
        "/api/bots/{slug}/attachments/{id}",
        get(crate::chat::attachments::download).delete(crate::chat::attachments::remove)
    );
    route!(
        "/api/bots/{slug}/requests/{id}/respond",
        post(respond_request)
    );
    route!(
        "/api/bots/{slug}/library",
        get(library_get).post(library_save)
    );
    route!(
        "/api/bots/{slug}/library/items/{id}",
        delete(library_delete)
    );
    route!("/api/bots/{slug}/library/file", get(library_file));
    route!("/api/bots/{slug}/shelf", get(shelf_get));
    route!(
        "/api/bots/{slug}/connectors",
        post(connector_save).delete(connector_delete)
    );
    route!("/api/providers", get(providers_get));
    route!("/api/providers/limits", get(providers_limits));
    route!("/api/providers/refresh", post(providers_refresh));
    route!("/api/suggestions", get(suggestions_get));
    route!("/api/bots/{slug}/model", get(model_get).put(model_put));
    route!("/api/bots/{slug}/usage", get(bot_usage));
    route!(
        "/api/bots/{slug}/routines",
        get(bot_routines).post(bot_routine_save)
    );
    route!(
        "/api/bots/{slug}/routines/enabled",
        post(bot_routine_enabled)
    );
    route!("/api/bots/{slug}/routines/delete", post(bot_routine_delete));
    route!("/api/bots/{slug}/routines/run", post(bot_routine_run));
    route!("/api/bots/{slug}/routines/{id}/hook", get(bot_hook));
    route!("/api/bots/{slug}/routines/{id}/rotate", post(bot_rotate));
    route!(
        "/api/bots/{slug}/routines/{id}/test",
        post(bot_routine_test)
    );
    route!(
        "/api/bots/{slug}/routines/{id}/deliveries",
        get(bot_deliveries)
    );
    route!(
        "/api/bots/{slug}/routines/{id}/deliveries/{delivery_id}",
        get(bot_delivery)
    );
    route!("/api/bots/{slug}/lineage", get(lineage_get));
    route!("/api/bots/{slug}/code", get(code_get));
    route!("/api/memory", get(memory_get).put(memory_put));
    route!(
        "/api/bots/{slug}/environment",
        get(environment_get).put(environment_put)
    );
    route!("/api/machines", get(machines_get));
    route!("/api/bots/{slug}/computer", get(computer_get));
    route!("/api/bots/{slug}/computer/open", post(computer_open));
    route!("/api/bots/{slug}/computer/close", post(computer_close));
    route!("/api/bots/{slug}/computer/control", post(computer_control));
    route!("/api/bots/{slug}/computer/input", post(computer_input));
    route!("/api/main-bot", put(main_bot));
    route!("/api/vault", get(vault_list).post(vault_create));
    route!(
        "/api/vault/{id}",
        get(vault_get).put(vault_update).delete(vault_delete)
    );
    route!(
        "/api/vault/import",
        post(crate::vault::vault_csv::import).layer(axum::extract::DefaultBodyLimit::max(8 << 20))
    );
    route!(
        "/api/vault/import-csv",
        post(crate::vault::vault_csv::import_csv)
            .layer(axum::extract::DefaultBodyLimit::max(8 << 20))
    );
    route!(
        "/api/vault/export.csv",
        get(crate::vault::vault_csv::export)
    );
    route!("/api/access", get(access_status));
    route!("/api/access/password", put(access_password));
    route!("/api/access/network", put(access_network));
    route!("/api/access/network/restart", post(access_network_restart));
    route!(
        "/api/storage",
        get(storage::settings::get_storage).put(storage::settings::put_storage)
    );
    route!("/api/storage/test", post(storage::settings::test_storage));
    route!("/api/storage/copy", post(storage::settings::copy_storage));
    route!(
        "/api/storage/restart",
        post(storage::settings::restart_storage)
    );
    route!(
        "/api/storage/key",
        put(storage::settings::put_storage_key).delete(storage::settings::delete_storage_key)
    );
    route!(
        "/api/storage/anytype/pair",
        post(storage::settings::pair_start)
    );
    route!(
        "/api/storage/anytype/pair/confirm",
        post(storage::settings::pair_confirm)
    );
    route!(
        "/api/storage/anytype/check",
        post(storage::settings::anytype_check)
    );
    route!(
        "/api/storage/anytype/setup",
        post(storage::settings::anytype_setup)
    );
    route!("/api/sync/status", get(storage::settings::get_sync_status));
    route!("/api/sync/run", post(storage::settings::post_sync_run));
    route!(
        "/api/sync/import",
        post(storage::settings::post_sync_import)
    );
    route!(
        "/api/sync/conflicts",
        get(storage::settings::get_sync_conflicts)
    );
    route!(
        "/api/sync/conflicts/{id}/resolve",
        post(storage::settings::post_sync_conflict_resolve)
    );
    route!("/api/access/tailscale", post(access_tailscale));
    route!("/api/access/tunnel", post(access_tunnel));
    route!("/api/thread-previews", post(thread_previews));
    route!("/api/rooms", get(rooms_list).post(rooms_create));
    route!(
        "/api/rooms/{id}",
        get(room_get).put(room_update).delete(room_delete)
    );
    route!("/api/rooms/{id}/messages", post(room_send));
    route!("/api/rooms/{id}/seen", post(room_seen));
    route!("/api/threads/{thread}/meta", get(thread_meta_get));
    route!("/api/threads/{thread}/reactions", post(thread_reaction));
    route!("/api/threads/{thread}/pins", post(thread_pin));
    route!("/api/threads/{thread}/reply", post(thread_reply));
    route!("/api/threads/{thread}/read", post(thread_read));
    route!("/api/threads/{thread}/voice", post(thread_voice));
    route!("/api/routines", get(routines_list).put(routines_upsert));
    route!("/api/routines/enabled", post(routines_enabled));
    route!("/api/routines/delete", post(routines_delete));
    route!("/api/routines/run", post(routines_run));
    route!("/api/icons/{host}", get(crate::bots::icons::get_icon));
    route!("/api/usage", get(crate::bots::usage::get_usage));
    route!("/api/usage/report", get(usage_report));
    route!("/api/system", get(crate::platform::release::system_status));
    route!(
        "/api/system/auto-update",
        put(crate::platform::release::set_auto_update)
    );
    route!(
        "/api/system/update-notice",
        delete(crate::platform::release::dismiss_notice)
    );
    route!(
        "/api/system/update",
        post(crate::platform::release::start_update)
    );
    (router, operations)
}
