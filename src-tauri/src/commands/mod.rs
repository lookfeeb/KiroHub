// 命令模块

#[path = "shared/common.rs"]
pub mod common;

#[path = "accounts/account_cmd/mod.rs"]
pub mod account_cmd;
#[path = "accounts/account_models.rs"]
pub(crate) mod account_models;
#[path = "app/app_data_cmd.rs"]
pub mod app_data_cmd;
#[path = "settings/app_settings_cmd.rs"]
pub mod app_settings_cmd;
#[path = "auth/auth_cmd.rs"]
pub mod auth_cmd;
#[path = "app/cache_cmd.rs"]
pub mod cache_cmd;
#[path = "kiro/cli_config_cmd.rs"]
pub mod cli_config_cmd;
#[path = "gateway/gateway_cmd.rs"]
pub mod gateway_cmd;
#[path = "accounts/group_tag_cmd.rs"]
pub mod group_tag_cmd;
#[path = "kiro/kiro_cli_cmd.rs"]
pub mod kiro_cli_cmd;
#[path = "kiro/kiro_settings_cmd.rs"]
pub mod kiro_settings_cmd;
#[path = "auth/machine_guid/mod.rs"]
pub mod machine_guid;
#[path = "mcp/mcp_cmd.rs"]
pub mod mcp_cmd;
#[path = "mcp/mcp_oauth_cmd.rs"]
pub mod mcp_oauth_cmd;
#[path = "app/session_manager.rs"]
pub mod session_manager;
#[path = "app/update_cmd.rs"]
pub mod update_cmd;
