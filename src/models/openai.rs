use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::runtime::Runtime;
use std::time::Duration;
use std::collections::HashSet;

// OpenAI API 配置结构
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub api_base_url: String,
    pub custom_models: Vec<CustomModel>,
}

// 自定义模型结构
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomModel {
    pub name: String,
    pub model_id: String,
    pub description: Option<String>,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gpt-3.5-turbo".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            system_prompt: "你是一个翻译助手，请帮助用户完成翻译任务。".to_string(),
            api_base_url: "https://api.openai.com/v1".to_string(),
            custom_models: Vec::new(),
        }
    }
}

// 消息结构，用于聊天模型
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

// ChatCompletion 请求结构
#[derive(Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f32,
    pub max_tokens: u32,
}

// API 响应结构
#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    #[allow(dead_code)]
    pub id: Option<String>,
    #[allow(dead_code)]
    pub object: Option<String>,
    #[allow(dead_code)]
    pub created: Option<u64>,
    #[allow(dead_code)]
    pub model: Option<String>,
    pub choices: Vec<ChatCompletionChoice>,
    #[allow(dead_code)]
    pub usage: Option<ChatCompletionUsage>,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionChoice {
    #[allow(dead_code)]
    pub index: u32,
    pub message: Message,
    #[allow(dead_code)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionUsage {
    #[allow(dead_code)]
    pub prompt_tokens: u32,
    #[allow(dead_code)]
    pub completion_tokens: u32,
    #[allow(dead_code)]
    pub total_tokens: u32,
}

#[derive(Debug)]
pub struct OpenAIClient {
    config: OpenAIConfig,
    runtime: Arc<Runtime>,
}

impl OpenAIClient {
    // 创建一个新的OpenAI客户端
    pub fn new(config: OpenAIConfig) -> Self {
        // 创建一个tokio运行时以支持异步操作
        let runtime = Runtime::new().expect("Failed to create tokio runtime");
        
        Self {
            config,
            runtime: Arc::new(runtime),
        }
    }
    
    // 设置或更新配置
    #[allow(dead_code)]
    pub fn set_config(&mut self, config: OpenAIConfig) {
        self.config = config;
    }
    
    // 获取当前配置的克隆
    #[allow(dead_code)]
    pub fn get_config(&self) -> OpenAIConfig {
        self.config.clone()
    }
    
    // 更新API密钥
    #[allow(dead_code)]
    pub fn set_api_key(&mut self, api_key: String) {
        self.config.api_key = api_key;
    }
    
    // 更新模型
    #[allow(dead_code)]
    pub fn set_model(&mut self, model: String) {
        self.config.model = model;
    }
    
    // 更新温度值
    #[allow(dead_code)]
    pub fn set_temperature(&mut self, temperature: f32) {
        self.config.temperature = temperature;
    }
    
    // 更新系统提示词
    #[allow(dead_code)]
    pub fn set_system_prompt(&mut self, system_prompt: String) {
        self.config.system_prompt = system_prompt;
    }
    
    // 异步发送聊天完成请求
    pub async fn async_chat_completion(&self, user_prompt: &str) -> Result<String, String> {
        let client = reqwest::Client::new();
        
        // 构建消息列表
        let messages = vec![
            Message {
                role: "system".to_string(),
                content: self.config.system_prompt.clone(),
            },
            Message {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ];
        
        // 构建请求体
        let request_body = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };
        
        // 构建API URL
        let url = format!("{}/chat/completions", self.config.api_base_url);
        
        // 发送请求
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&request_body)
            .timeout(Duration::from_secs(60)) // 设置60秒超时
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;
        
        // 检查状态码
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "无法获取错误详情".to_string());
            return Err(format!("API错误 ({}): {}", status, error_text));
        }
        
        // 解析响应
        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;
        
        // 获取返回的文本
        if let Some(choice) = completion.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err("API返回了空响应".to_string())
        }
    }
    
    // 同步包装器，用于在同步上下文中调用异步函数
    pub fn chat_completion(&self, user_prompt: &str) -> Result<String, String> {
        self.runtime.block_on(self.async_chat_completion(user_prompt))
    }
    
    // 翻译一个字符串
    pub fn translate(&self, text: &str, source_lang: &str, target_lang: &str) -> Result<String, String> {
        let prompt = format!(
            "请将以下{}翻译成{}，只返回翻译结果，不要添加任何解释或格式化：\n\n{}",
            source_lang, target_lang, text
        );
        
        self.chat_completion(&prompt)
    }
    
    // 检查API密钥是否有效
    #[allow(dead_code)]
    pub fn check_api_key(&self) -> bool {
        if self.config.api_key.is_empty() {
            return false;
        }
        
        // 尝试发送一个简单的请求来检查API密钥
        match self.chat_completion("Hello, this is a test message. Please respond with 'API key is valid'.") {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

// 可用模型列表
#[allow(dead_code)]
pub fn available_models() -> Vec<String> {
    vec![
        "gpt-3.5-turbo".to_string(),
        "gpt-3.5-turbo-16k".to_string(),
        "gpt-4".to_string(),
        "gpt-4-turbo".to_string(),
        "gpt-4o".to_string(),
    ]
}

// 获取所有模型列表（内置+自定义）
pub fn get_all_models(config: &OpenAIConfig) -> Vec<String> {
    let mut models = available_models();
    
    // 添加自定义模型，确保没有重复
    let mut used_model_ids = HashSet::new();
    for model in &models {
        used_model_ids.insert(model.clone());
    }
    
    for custom_model in &config.custom_models {
        if !used_model_ids.contains(&custom_model.model_id) {
            models.push(custom_model.model_id.clone());
            used_model_ids.insert(custom_model.model_id.clone());
        }
    }
    
    models
}

// 通过模型ID获取自定义模型
#[allow(dead_code)]
pub fn get_custom_model_by_id(config: &OpenAIConfig, model_id: &str) -> Option<CustomModel> {
    config.custom_models.iter()
        .find(|m| m.model_id == model_id)
        .cloned()
} 