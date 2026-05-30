pub(crate) use crate::gateway::models::{
    AnthropicMessagesRequest, ConversationState, CurrentMessage, HistoryAssistantMessage,
    HistoryItem, HistoryUserMessage, ImageBlock, ImageSource, KiroInputSchema,
    KiroPayload, KiroTool, KiroToolResult, KiroToolResultContent, KiroToolSpec, KiroToolUse,
    ModelInfo, NormalizedMessage, NormalizedRequest, OpenAIChatRequest,
    Tool, ToolCall, ToolCallFunction, ToolFunction, UserInputMessage,
    UserInputMessageContext,
};
pub(crate) use base64::{engine::general_purpose::STANDARD, Engine};
pub(crate) use reqwest::Client;
pub(crate) use serde_json::{json, Map, Value};
pub(crate) use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};
pub(crate) use tokio::net::lookup_host;
pub(crate) use uuid::Uuid;

pub const TOOL_DESCRIPTION_MAX_LENGTH: usize = 10237;
pub(crate) const MAX_IMAGE_SOURCE_BYTES: usize = 5 * 1024 * 1024;
pub(crate) const MAX_IMAGE_REDIRECTS: usize = 3;
pub(crate) const IMAGE_FETCH_TIMEOUT_SECONDS: u64 = 15;

mod entry;
mod openai_chat;
mod responses;
mod anthropic;
mod model;
mod payload;
mod tools;
mod history;
mod content;
mod image;
mod util;

pub(crate) use entry::*;
pub(crate) use openai_chat::*;
pub(crate) use responses::*;
pub(crate) use anthropic::*;
pub(crate) use model::*;
pub(crate) use payload::*;
pub(crate) use tools::*;
pub(crate) use history::*;
pub(crate) use content::*;
pub(crate) use image::*;
pub(crate) use util::*;

#[cfg(test)]
mod tests;
