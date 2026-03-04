use serde::{Deserialize, Serialize};

use super::executor::{DirEntry, ExecResult, FileContent};

/// Protocol messages sent over QUIC between core and remote nodes.
#[derive(Debug, Serialize, Deserialize)]
pub enum ProtocolMessage {
    // Auth
    AuthRequest { code: String },
    AuthResponse { success: bool, token: Option<String> },
    TokenAuth { token: String },
    TokenAuthResponse { success: bool },

    // Commands
    RunCommand { command: String, cwd: Option<String> },
    RunCommandResult(ExecResult),

    ReadFile { path: String },
    ReadFileResult(FileContent),

    WriteFile { path: String, content: String },
    WriteFileResult { success: bool, error: Option<String> },

    ListDir { path: String },
    ListDirResult(Vec<DirEntry>),

    SearchFiles { pattern: String, root: Option<String> },
    SearchFilesResult(Vec<String>),

    // Errors
    Error { message: String },

    // Ping/Pong for keepalive
    Ping,
    Pong,
}
