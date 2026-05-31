#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 核心模块
mod core;
mod db;
mod state;

// 功能模块
mod auth;
mod auto_switch;
mod model_lock;
mod clients;
mod commands;
mod gateway;
mod kiro;
mod mcp_oauth;
mod mcp_proxy;
mod models;
mod services;
mod tasks;  // 后台任务模块
mod utils;

use core::account::{AccountStore, GroupTagStore};
use auth::AuthState;
use state::AppState;
use std::sync::Mutex;
use tauri::Listener;
use services::session_storage::SessionStorage;

// 导入命令
use utils::browser::{detect_installed_browsers};

//账号管理页面
use commands::account_cmd::{
    add_account_by_idc, add_account_by_social, add_local_kiro_account, delete_account,
    delete_account_remote, delete_accounts, export_accounts, get_account_usage, get_accounts,
    get_accounts_by_group, get_accounts_by_tag, get_available_accounts, import_accounts,
    list_available_models, refresh_account_token, set_overage_status, sync_account, update_account, verify_account,
    check_token_status, check_all_tokens_status, refresh_all_expiring_tokens,
};
//应用设置
use commands::app_settings_cmd::{
    bind_machine_id_to_account, get_all_bound_machine_ids, get_app_settings, get_bound_machine_id,
    get_usage_history, save_app_settings, save_usage_history_entry, unbind_machine_id_from_account,
    get_custom_kiro_path, set_custom_kiro_path, clear_custom_kiro_path,
    get_current_cli_account_id,
};
use commands::app_data_cmd::{backup_app_database, get_app_data_dir, open_app_data_dir};
//授权相关
use commands::auth_cmd::{
    cancel_kiro_login, get_current_user, get_supported_providers, handle_kiro_social_callback,
    kiro_login, logout,
};
use commands::cli_config_cmd::{
    check_claude_code_installed, check_codex_cli_installed, write_claude_code_config,
    write_codex_cli_config,
};

//网关反代
use commands::gateway_cmd::{
    clear_gateway_request_logs, configure_proxy_clients, get_gateway_config, get_gateway_log_dir, get_gateway_request_logs,
    get_gateway_request_stats, get_gateway_model_stats, get_gateway_endpoint_stats,
    get_gateway_status, open_gateway_log_dir, save_gateway_config, start_gateway, stop_gateway,
};
//缓存管理
use commands::cache_cmd::{
    get_cache_config, get_cache_stats, clear_all_cache, clear_session_cache, cleanup_expired_cache,
};
//分组
use commands::group_tag_cmd::{
    add_group, add_tag, add_tag_to_account, delete_group, delete_tag, get_groups, get_tags,
    remove_account_tags, remove_tag_from_account, reorder_groups, set_account_group,
    set_account_tags, update_group, update_tag,
};
//kiro-cli
use commands::kiro_cli_cmd::{
    check_cli_installation, get_kiro_cli_default_path, import_from_kiro_cli,
    read_cli_db_snapshot, rollback_cli_switch, switch_to_cli_account,
};
use commands::kiro_settings_cmd::{
    get_kiro_settings, set_kiro_agent_autonomy, set_kiro_codebase_indexing, set_kiro_configure_mcp,
    set_kiro_debug_logs, set_kiro_model, set_kiro_notification, set_kiro_proxy,
    set_kiro_reference_tracker, set_kiro_tab_autocomplete, set_kiro_telemetry,
    set_kiro_trusted_commands, set_kiro_trusted_tools, set_kiro_usage_summary,
};
use commands::machine_guid::{
    clear_macos_override, generate_machine_guid,
    get_system_machine_guid, reset_system_machine_guid, restart_as_admin,
    set_custom_machine_guid,
};
use commands::mcp_cmd::{
    delete_mcp_server, delete_mcp_server_synced, discover_and_import_mcp_servers, get_mcp_config,
    get_mcp_tool_stats, save_mcp_server, toggle_mcp_server,
};
use commands::mcp_oauth_cmd::{mcp_oauth_authorize, mcp_oauth_revoke, mcp_oauth_status};

use commands::session_manager::{
    list_workspaces, list_sessions, load_session, delete_session, delete_workspace, export_session, search_sessions, get_session_file_path, refresh_session_cache,
};


//代理
use commands::proxy_cmd::detect_system_proxy;

//Kiro IDE
use crate::kiro::ide::{
    check_ide_installation, check_kiro_config_files,
    get_kiro_local_token, read_kiro_accounts, switch_kiro_account,
};
//Kiro进程
use crate::kiro::process::{close_kiro_ide, is_kiro_ide_running, start_kiro_ide};

use commands::update_cmd::check_update;

/// 配置日志插件
fn setup_log_plugin() -> tauri_plugin_log::Builder {
    let log_level = gateway::load_gateway_config()
        .ok()
        .map(|config| match config.log_level.as_str() {
            "info" => log::LevelFilter::Info,
            "warn" => log::LevelFilter::Warn,
            "error" => log::LevelFilter::Error,
            _ => log::LevelFilter::Debug,
        })
        .unwrap_or(log::LevelFilter::Debug);

    tauri_plugin_log::Builder::new()
        .level(log_level)
        // 只显示我们自己的日志，过滤掉第三方库的日志
        .filter(|metadata| {
            let target = metadata.target();
            target.starts_with("kiro_hub")
        })
        // 紧凑格式器：HH:MM:SS LEVEL [短模块] msg
        // 默认格式 [date][time][full_target][LEVEL] msg 在终端太长不利于阅读
        .format(|out, message, record| {
            let target = record.target();
            // 取最后一段模块名
            let short_target = target.rsplit("::").next().unwrap_or(target);
            out.finish(format_args!(
                "{} {} [{}] {}",
                chrono::Local::now().format("%H:%M:%S"),
                record.level(),
                short_target,
                message
            ))
        })
        // 输出到日志文件 + 终端 stdout（dev 模式可见）
        // 文件名：app.log（所有模块通用应用日志，含 [Gateway]/[Account] 等前缀区分）
        // Gateway 业务请求单独写 gateway-*.log
        .targets([
            tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::LogDir { file_name: Some("app".to_string()) }
            ),
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
        ])
        // 日志轮转：每个文件最大 10MB，保留最近 5 个文件
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
        .max_file_size(10_000_000)
}

/// 配置单实例插件回调
#[allow(clippy::needless_pass_by_value)] // Tauri 框架要求回调签名为 Vec<String>
fn setup_single_instance_callback(app: &tauri::AppHandle, argv: Vec<String>, _cwd: String) {
    // 当第二个实例尝试启动时，处理传入的参数（deep-link 回调）
    let protocol_prefix = format!(
        "{}://",
        core::deep_link_handler::DeepLinkCallbackWaiter::get_protocol_scheme()
    );
    for arg in &argv {
        if arg.starts_with(&protocol_prefix) {
            auth::handle_incoming_deep_link(app, arg);
        }
    }
}

/// 处理 deep link 事件
fn handle_deep_link_event(app_handle: &tauri::AppHandle, payload: &str) {
    // payload 可能是 JSON 格式 ["kiro-account-manager://..."] 或纯 URL
    let url = if payload.starts_with('[') {
        // JSON 数组格式，解析第一个元素
        serde_json::from_str::<Vec<String>>(payload)
            .ok()
            .and_then(|v| v.into_iter().next())
            .unwrap_or_else(|| payload.to_string())
    } else if payload.starts_with('"') {
        // JSON 字符串格式
        serde_json::from_str::<String>(payload).unwrap_or_else(|_| payload.to_string())
    } else {
        payload.to_string()
    };

    // 只处理当前环境的协议
    let protocol_prefix = format!(
        "{}://",
        core::deep_link_handler::DeepLinkCallbackWaiter::get_protocol_scheme()
    );
    if !url.starts_with(&protocol_prefix) {
        return;
    }

    auth::handle_incoming_deep_link(app_handle, &url);
}

/// 应用 setup 回调
fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 deep link 处理器
    core::deep_link_handler::init();

    // 首次启动时检查命令行参数中的 deep link（Windows/Linux）
    let protocol_prefix = format!(
        "{}://",
        core::deep_link_handler::DeepLinkCallbackWaiter::get_protocol_scheme()
    );
    for arg in std::env::args() {
        if arg.starts_with(&protocol_prefix) {
            auth::handle_incoming_deep_link(app.handle(), &arg);
        }
    }

    // 监听 deep link 事件（根据环境自动选择协议）
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        use tauri_plugin_deep_link::DeepLinkExt;
        let scheme = core::deep_link_handler::DeepLinkCallbackWaiter::get_protocol_scheme();
        app.deep_link()
            .register(scheme)
            .map_err(|e| format!("Failed to register deep link: {e}"))?;
    }

    // 监听 deep link URL
    let app_handle = app.handle().clone();
    app.listen("deep-link://new-url", move |event| {
        handle_deep_link_event(&app_handle, event.payload());
    });

    // 启动时确保网关至少有一个客户端 API Key：没有则自动生成
    if let Err(err) = gateway::ensure_client_api_key_exists() {
        log::warn!("自动生成网关 API Key 失败: {err}");
    }

    // 自动启动网关（如果配置了）
    let app_handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = gateway::auto_start_if_enabled(&app_handle).await {
            log::error!("自动启动反代失败: {err}");
        }
    });

    // 启动模型锁定后台任务
    let app_handle = app.handle().clone();
    model_lock::start_model_lock_task(app_handle);

    // 启动自动换号后台任务
    let app_handle = app.handle().clone();
    auto_switch::start_auto_switch_task(app_handle);

    // 启动 Token 自动刷新后台任务（参考 Kiro IDE）
    tasks::token_refresh::start_token_refresh_loop(app.handle().clone());

    // 启动 MCP 本地反向代理与令牌自动刷新后台任务
    tauri::async_runtime::spawn(async {
        if let Err(e) = mcp_proxy::start_proxy().await {
            log::error!("MCP 反向代理启动失败: {e}");
        }
    });
    tasks::mcp_token_refresh::start_mcp_token_refresh_loop(app.handle().clone());

    // 创建托盘图标
    setup_system_tray(app)?;

    // 监听窗口关闭事件
    setup_window_close_handler(app)?;

    // 主窗口由前端首屏 ready 后通过命令触发显示，避免 setup 阶段过早白屏

    Ok(())
}

/// 创建系统托盘
fn setup_system_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
        Manager,
    };

    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("KiroHub")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// 监听窗口关闭事件
fn setup_window_close_handler(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    
    if let Some(window) = app.get_webview_window("main") {
        let window_clone = window.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 读取配置，决定是最小化到托盘还是直接退出
                let settings = commands::app_settings_cmd::get_app_settings_inner()
                    .unwrap_or_default();

                if settings.close_to_tray.unwrap_or(false) {
                    // 最小化到托盘
                    api.prevent_close();
                    let _ = window_clone.hide();
                } else {
                    // 直接退出
                    // 不调用 api.prevent_close()，让窗口正常关闭
                }
            }
        });
    }

    Ok(())
}

#[allow(clippy::too_many_lines)] // Tauri 框架要求在 main 中注册所有命令，无法拆分
/// 将旧数据目录 `.kiro-account-manager` 自动迁移到新目录 `.kirohub`
/// （仅当新目录不存在且旧目录存在时执行整目录重命名，保留全部账号与设置）
fn migrate_data_dir() {
    if let Some(base) = dirs::data_dir() {
        let old = base.join(".kiro-account-manager");
        let new = base.join(".kirohub");
        if old.exists() && !new.exists() {
            match std::fs::rename(&old, &new) {
                Ok(_) => eprintln!("[Migrate] 数据目录已迁移: {} -> {}", old.display(), new.display()),
                Err(e) => eprintln!("[Migrate] 数据目录迁移失败: {e}"),
            }
        }
    }
}

fn main() {
    // 启动最早期执行：把旧数据目录迁移到新目录（须在 AccountStore::new() 读盘之前）
    migrate_data_dir();
    // 初始化 SQLite 全局连接池（须在 AccountStore::new() / GroupTagStore::new() 之前）
    if let Err(e) = db::init_global() {
        eprintln!("[DB] 全局连接池初始化失败: {e}");
    }
    tauri::Builder::default()
        .plugin(setup_log_plugin().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_http::init())
        // 单实例插件：确保只有一个实例运行，deep-link 回调传递给已运行的实例
        .plugin(tauri_plugin_single_instance::init(
            setup_single_instance_callback,
        ))
        .manage(AppState {
            store: Mutex::new(AccountStore::new()),
            group_tag_store: Mutex::new(GroupTagStore::new()),
            auth: AuthState::new(),
            pending_login: Mutex::new(None),
            gateway: Mutex::new(None),
        })
        .manage(SessionStorage::new().expect("Failed to initialize SessionStorage"))
        .setup(setup_app)
        .invoke_handler(tauri::generate_handler![
            // 账号命令
            get_accounts,
            delete_account,
            delete_accounts,
            delete_account_remote,
            update_account,
            sync_account,
            refresh_account_token,
            verify_account,
            add_account_by_social,
            add_local_kiro_account,
            add_account_by_idc,
            import_accounts,
            export_accounts,
            list_available_models,
            get_available_accounts,
            get_accounts_by_group,
            get_accounts_by_tag,
            get_account_usage,
            set_overage_status,
            // Token 状态检查命令
            check_token_status,
            check_all_tokens_status,
            refresh_all_expiring_tokens,
            // Kiro CLI 导入命令
            get_kiro_cli_default_path,
            import_from_kiro_cli,
            check_cli_installation,
            read_cli_db_snapshot,
            switch_to_cli_account,
            rollback_cli_switch,
            // 分组与标签命令
            get_groups,
            add_group,
            update_group,
            delete_group,
            reorder_groups,
            get_tags,
            add_tag,
            update_tag,
            delete_tag,
            set_account_group,
            add_tag_to_account,
            remove_tag_from_account,
            set_account_tags,
            remove_account_tags,
            // Auth 命令
            get_current_user,
            logout,
            cancel_kiro_login,
            kiro_login,
            get_supported_providers,
            handle_kiro_social_callback,
            check_ide_installation,
            check_kiro_config_files,
            get_kiro_local_token,
            read_kiro_accounts,
            switch_kiro_account,
            set_custom_kiro_path,
            get_custom_kiro_path,
            clear_custom_kiro_path,
            // 进程管理命令
            close_kiro_ide,
            start_kiro_ide,
            is_kiro_ide_running,
            // Kiro IDE 设置命令（仅保留 model_lock 依赖）
            get_kiro_settings,
            set_kiro_model,
            set_kiro_proxy,
            set_kiro_codebase_indexing,
            set_kiro_trusted_commands,
            set_kiro_agent_autonomy,
            set_kiro_tab_autocomplete,
            set_kiro_usage_summary,
            set_kiro_debug_logs,
            set_kiro_trusted_tools,
            set_kiro_reference_tracker,
            set_kiro_configure_mcp,
            set_kiro_notification,
            set_kiro_telemetry,
            // 应用设置命令
            get_app_settings,
            save_app_settings,
            get_current_cli_account_id,
            // 使用量历史记录命令
            get_usage_history,
            save_usage_history_entry,
            // 账号绑定机器码命令
            bind_machine_id_to_account,
            unbind_machine_id_from_account,
            get_bound_machine_id,
            get_all_bound_machine_ids,
            // 系统机器码命令
            get_system_machine_guid,
            reset_system_machine_guid,
            set_custom_machine_guid,
            clear_macos_override,
            generate_machine_guid,
            restart_as_admin,
            // 浏览器检测
            detect_installed_browsers,
            // MCP 管理命令
            get_mcp_config,
            save_mcp_server,
            delete_mcp_server,
            toggle_mcp_server,
            get_mcp_tool_stats,
            discover_and_import_mcp_servers,
            delete_mcp_server_synced,
            mcp_oauth_authorize,
            mcp_oauth_status,
            mcp_oauth_revoke,
            // Gateway 命令
            start_gateway,
            stop_gateway,
            get_gateway_status,
            get_gateway_config,
            save_gateway_config,
            get_gateway_log_dir,
            get_gateway_request_logs,
            get_gateway_request_stats,
            get_gateway_model_stats,
            get_gateway_endpoint_stats,
            open_gateway_log_dir,
            clear_gateway_request_logs,
            configure_proxy_clients,
            // 缓存管理命令
            get_cache_config,
            get_cache_stats,
            clear_all_cache,
            clear_session_cache,
            cleanup_expired_cache,
            // 代理检测命令
            detect_system_proxy,
            // 更新检查命令
            check_update,
            // Session Manager 命令
            list_workspaces,
            list_sessions,
            load_session,
            get_session_file_path,
            delete_session,
            delete_workspace,
            export_session,
            search_sessions,
            refresh_session_cache,
            // CLI 配置命令
            check_claude_code_installed,
            check_codex_cli_installed,
            write_claude_code_config,
            write_codex_cli_config,
            // 应用数据目录命令
            get_app_data_dir,
            open_app_data_dir,
            backup_app_database
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
