pub fn check_for_updates() -> anyhow::Result<()> {
    // Call tx3up subprocess to check for updates
    let output = std::process::Command::new("tx3up")
        .args(["check", "--output", "json"])
        .output();

    // If there's any error, abort silently
    let output = match output {
        Ok(output) => output,
        Err(_) => return Ok(()),
    };

    // Check if the command was successful
    if !output.status.success() {
        return Ok(());
    }

    // Parse the JSON output
    let json_str = match String::from_utf8(output.stdout) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };

    // Parse as JSON array
    let updates: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&json_str);
    let updates = match updates {
        Ok(updates) => updates,
        Err(_) => return Ok(()),
    };

    // If there are updates available, print a message
    if updates.len() > 0 {
        println!("\n⚠️ Updates available! Run 'tx3up' to install them.\n");
    }

    Ok(())
}
