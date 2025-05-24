use anyhow::{Context, Result};
use clap::Parser;
use dotenv::dotenv;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(
    name = "gAi",
    author = "Ian Irizarry",
    version = "0.1.0",
    about = "Generate AI-powered git commit messages from your diffs",
    long_about = None
)]
struct Args {
    /// Generate a commit message from staged changes
    #[arg(short, long)]
    generate: bool,

    /// Generate and immediately commit with the message
    #[arg(short, long)]
    commit: bool,

    /// Model to use (default: o4-mini)
    #[arg(short, long, default_value = "gpt-4.1-nano")]
    model: String,

    /// Temperature for generation (0.0-2.0, default: 1)
    #[arg(short, long, default_value = "1")]
    temperature: f32,
}

#[derive(Serialize, Debug)]
struct OpenAIRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
}

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct OpenAIResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    error: Option<OpenAIError>,
}

#[derive(Deserialize, Debug)]
struct Choice {
    message: Message,
}

#[derive(Deserialize, Debug)]
struct OpenAIError {
    message: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    dotenv().ok();
    
    // Parse command line arguments
    let args = Args::parse();
    
    if args.generate || args.commit {
        // Generate commit message
        let commit_message = generate_commit_message(&args.model, args.temperature).await?;
        
        if args.commit {
            // Use the generated message to create a commit
            create_commit(&commit_message)?;
            println!("✅ Committed with message: \"{}\"", commit_message);
        } else {
            // Just print the message
            println!("📝 Generated commit message:");
            println!("{}", commit_message);
            println!("\nTo use this message:");
            println!("git commit -m \"{}\"", commit_message);
        }
    } else {
        println!("🤖 gAi - AI Powered Git Commit Messages");
        println!("Use --generate (-g) to create a commit message");
        println!("Use --commit (-c) to commit with the generated message");
        println!("\nRun 'gai --help' for more options");
    }
    
    Ok(())
}

async fn generate_commit_message(model: &str, temperature: f32) -> Result<String> {
    // Get OpenAI API key from environment variables
    let api_key = env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not found. Please set it in your .env file or environment variables.")?;
    
    // Get git diff
    let diff = get_git_diff()?;
    
    // Create OpenAI API client
    let client = Client::new();
    
    // Create the request body
    let request = OpenAIRequest {
        model: model.to_string(),
        messages: vec![
    Message {
        role: "system".to_string(),
        content: "You are an expert at writing conventional git commit messages. Analyze code diffs and generate a single, concise commit message following the format: <type>[optional scope]: <description>
                    COMMIT TYPES:
                    - **feat**: A new feature for the user
                    - **fix**: A bug fix  
                    - **docs**: Documentation only changes
                    - **style**: Changes that don't affect code meaning (whitespace, formatting, semicolons)
                    - **refactor**: Code change that neither fixes a bug nor adds a feature
                    - **test**: Adding missing tests or correcting existing tests
                    - **chore**: Changes to build process, auxiliary tools, or maintenance
                    - **perf**: Performance improvements
                    - **ci**: Changes to CI configuration files and scripts
                    - **build**: Changes affecting the build system or external dependencies
                    - **revert**: Reverts a previous commit

                    EXAMPLES:
                    feat: add user authentication system
                    feat(auth): implement password reset functionality
                    fix: resolve memory leak in data processing
                    fix(api): handle null response from external service
                    docs: update API documentation
                    docs(readme): add installation instructions
                    style: fix indentation in user service
                    style(css): update button hover effects
                    refactor: extract validation logic into separate module
                    refactor(utils): simplify date formatting functions
                    test: add unit tests for payment processing
                    test(integration): add API endpoint tests
                    chore: update dependencies
                    chore(deps): bump lodash from 4.17.19 to 4.17.21
                    perf: improve database query efficiency
                    perf(images): optimize image loading algorithm
                    ci: add automated testing workflow
                    ci(github): update deployment pipeline
                    build: update webpack configuration
                    build(npm): add new build script
                    revert: revert \"feat: add experimental feature\"

                    RULES:
                    - Keep description under 50 characters when possible
                    - Use imperative mood (add, fix, update, not added, fixed, updated)
                    - Don't end with a period
                    - Focus on WHAT changed, not HOW
                    - If multiple types of changes, pick the most significant one
                    - Use scope in parentheses when appropriate (component, file, or area affected)".to_string(),
            },
            Message {
                role: "user".to_string(),
                content: format!("Generate a conventional commit message for this diff:\n\n{}", diff),
            },
        ],
        temperature,
    };
    
    // Send request to OpenAI API
    let response = client.post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .context("Failed to send request to OpenAI API")?;
    
    // Check if response status is successful
    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow::anyhow!("API request failed: {}", error_text));
    }
    
    // Parse response
    let response_body = response.json::<OpenAIResponse>()
        .await
        .context("Failed to parse OpenAI API response")?;
    
    // Check for API errors
    if let Some(error) = response_body.error {
        return Err(anyhow::anyhow!("OpenAI API error: {}", error.message));
    }
    
    // Extract commit message from response
    let commit_message = response_body.choices
        .first()
        .context("No choices in response")?
        .message
        .content
        .clone();
    
    // Clean up the message (remove quotes if present, trim whitespace)
    let clean_message = commit_message
        .trim()
        .trim_matches('"')
        .to_string();
    
    Ok(clean_message)
}

fn get_git_diff() -> Result<String> {
    // Check if we're in a git repository
    let git_check = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .context("Failed to execute git command. Is git installed?")?;
    
    if !git_check.status.success() {
        return Err(anyhow::anyhow!("Not inside a git repository"));
    }
    
    // Execute git diff --staged command and capture output
    let output = Command::new("git")
        .args(["diff", "--staged"])
        .output()
        .context("Failed to execute git diff command")?;
    
    // Convert output to string
    let diff = String::from_utf8(output.stdout)
        .context("Failed to parse git diff output as UTF-8")?;
    
    if diff.is_empty() {
        return Err(anyhow::anyhow!("No staged changes found. Use 'git add' to stage your changes."));
    }
    
    Ok(diff)
}

fn create_commit(message: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["commit", "-m", message])
        .output()
        .context("Failed to execute git commit command")?;
    
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Commit failed: {}", error));
    }
    
    Ok(())
}
