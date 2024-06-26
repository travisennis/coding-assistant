use std::fmt;

use serde::{Deserialize, Serialize};

pub enum Provider {
    Anthropic,
    OpenAI,
    Mistral,
    Google,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum Model {
    #[serde(rename = "gpt-4o")]
    GPT4o,
    #[serde(rename = "gpt-4-turbo-preview")]
    GPT4Turbo,
    #[serde(rename = "gpt-3-turbo")]
    GPT3Turbo,
    #[serde(rename = "claude-3-5-sonnet-20240620")]
    Claude3_5Sonnet,
    #[serde(rename = "claude-3-opus-20240229")]
    Claude3Opus,
    #[serde(rename = "claude-3-sonnet-20240229")]
    Claude3Sonnet,
    #[serde(rename = "claude-3-haiku-20240307")]
    Claude3Haiku,
    #[serde(rename = "codestral-latest")]
    Codestral,
    #[serde(rename = "gemini-1.5-flash-latest")]
    GeminiFlash,
    #[serde(rename = "gemini-1.5-pro-latest")]
    GeminiPro,
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::GPT4o => write!(f, "GPT-4o"),
            Self::GPT4Turbo => write!(f, "GPT-4-Turbo"),
            Self::GPT3Turbo => write!(f, "GPT-3-Turbo"),
            Self::Claude3Opus => write!(f, "Claude 3 Opus"),
            Self::Claude3Sonnet => write!(f, "Claude 3 Sonnet"),
            Self::Claude3Haiku => write!(f, "Claude 3 Haiku"),
            Self::Codestral => write!(f, "Codestral"),
            Self::Claude3_5Sonnet => write!(f, "Claude 3.5 Sonnet"),
            Self::GeminiFlash => write!(f, "Gemini 1.5 Flash"),
            Self::GeminiPro => write!(f, "Gemini 1.5 Pro"),
        }
    }
}

pub struct ProviderModel {
    pub provider: Provider,
    pub model: Model,
}

impl ProviderModel {
    pub fn get_or_default(model_name: &str, default: (Provider, Model)) -> Self {
        let result = match model_name {
            "gpt-4-turbo" => (Provider::OpenAI, Model::GPT4Turbo),
            "gpt-3-turbo" => (Provider::OpenAI, Model::GPT3Turbo),
            "sonnet" => (Provider::Anthropic, Model::Claude3_5Sonnet),
            "opus" => (Provider::Anthropic, Model::Claude3Opus),
            "sonnet3" => (Provider::Anthropic, Model::Claude3Sonnet),
            "haiku" => (Provider::Anthropic, Model::Claude3Haiku),
            "gemini-flash" => (Provider::Google, Model::GeminiFlash),
            "gemini-pro" => (Provider::Google, Model::GeminiPro),
            _ => default,
        };

        Self {
            provider: result.0,
            model: result.1,
        }
    }
}
