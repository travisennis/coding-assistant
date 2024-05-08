use std::error::Error;

use anyhow::Result;
use clap::Args;
use rustyline::{error::ReadlineError, DefaultEditor};
use termimad::MadSkin;

use crate::{
    cli::get_provider_model,
    clients::LLMClient,
    config::DataDir,
    errors::CAError,
    messages::{Message, Role},
};

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the model to use
    #[arg(long, default_value_t = String::from("gpt-4-turbo"))]
    model: String,

    /// Sets the temperature value
    #[arg(long, default_value_t = 0.0)]
    temperature: f32,

    /// Sets the max_tokens value
    #[arg(long, default_value_t = 1024)]
    max_tokens: u32,

    /// Sets the top_p value
    #[arg(long, default_value_t = 1.0)]
    top_p: f32,
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let context: Result<String, CAError> = {
            if atty::is(atty::Stream::Stdin) {
                Err(CAError::Input)
            } else {
                match std::io::read_to_string(std::io::stdin()) {
                    Ok(result) => Ok(result),
                    Err(_error) => Err(CAError::Input),
                }
            }
        };

        let provider_model = get_provider_model(&self.model);

        let system_prompt = "You are a helpful coding assistant. Provide answers in markdown format unless instructed otherwise. If the request is ambiguous, ask questions. If you don't know the answer, admit you don't.";

        let mut client = LLMClient::new(provider_model.0, provider_model.1, system_prompt);

        let mut messages: Vec<Message> = vec![];

        if let Ok(context) = context {
            messages.push(Message {
                role: Role::User,
                content: context,
            });
        }

        let mut rl = DefaultEditor::new().expect("Editor not initialized.");

        let skin = MadSkin::default();

        loop {
            let readline = rl.readline("> ");
            match readline {
                Ok(line) if line.trim() == "bye" => {
                    break;
                }
                Ok(line) => {
                    let user_msg = Message {
                        role: Role::User,
                        content: line,
                    };

                    messages.push(user_msg);

                    let response = client.send_message(&mut messages).await?;

                    if let Some(msg) = response {
                        println!("\n");
                        skin.print_text(&msg.content);
                        println!("\n");
                        messages.push(msg);
                    }
                }
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                    break;
                }
                Err(err) => {
                    println!("Error: {err:?}");
                    break;
                }
            }
        }

        DataDir::new().save_messages(&client.get_message_history());

        Ok(())
    }
}
