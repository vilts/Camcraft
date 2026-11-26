//! The `--mcp-port` flag of the app (#183), moved out of listen.rs
//! (#3751) to keep that file under the 400-line law.
//!
//! Provenance: our own argv parsing. Clean-room, public sources only.

/// `--mcp-port <n>` / `--mcp-port=<n>` over `CAMCRAFT_MCP_PORT`: `None`
/// when neither asks for the server (it is off by default).
///
/// # Errors
/// The flag without a value, or a value that is not a port number.
pub fn port_from(args: &[String], env: Option<&str>) -> Result<Option<u16>, String> {
    let flag = args.iter().enumerate().find_map(|(i, a)| {
        if a == "--mcp-port" {
            Some(args.get(i + 1).cloned())
        } else {
            a.strip_prefix("--mcp-port=").map(|v| Some(v.to_owned()))
        }
    });
    let text = match flag {
        Some(None) => return Err("--mcp-port needs a port number".into()),
        Some(Some(v)) => v,
        None => match env.map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => v.to_owned(),
            None => return Ok(None),
        },
    };
    text.trim()
        .parse::<u16>()
        .map(Some)
        .map_err(|_| format!("MCP port must be a number from 0 to 65535, got '{text}'"))
}
