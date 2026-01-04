use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub fn get_user_fingerprint() -> String {
    // Try to load from persistent storage first
    if let Some(stored) = load_stored_fingerprint() {
        stored
    } else {
        // Generate new one and store it
        let fingerprint = generate_user_fingerprint();
        store_fingerprint(&fingerprint);
        fingerprint
    }
}

fn generate_user_fingerprint() -> String {
    // Generate a stable but anonymous fingerprint
    let mut hasher = DefaultHasher::new();

    // Use machine-specific but non-identifying data
    if let Some(hostname) = get_hostname() {
        hostname.hash(&mut hasher);
    }

    // Use OS-specific non-identifying data
    if let Some(os_info) = get_os_info() {
        os_info.hash(&mut hasher);
    }

    // Add some randomness for uniqueness
    let random_component = generate_stable_random();
    random_component.hash(&mut hasher);

    // Format as hex string
    format!("{:x}", hasher.finish())
}

fn get_hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| {
            // Try to get hostname from system
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
        })
}

fn get_os_info() -> Option<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    Some(format!("{}-{}", os, arch))
}

fn generate_stable_random() -> u64 {
    // Use a combination of process ID and current time for some randomness
    let pid = std::process::id();
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    // Simple hash to combine them
    let mut hasher = DefaultHasher::new();
    pid.hash(&mut hasher);
    time.hash(&mut hasher);
    hasher.finish()
}

fn load_stored_fingerprint() -> Option<String> {
    // Load from ~/.tx3/trix/fingerprint
    let mut path = crate::home::tx3_dir().ok()?;
    path.push("trix/fingerprint");
    std::fs::read_to_string(path).ok()
}

fn store_fingerprint(fingerprint: &str) {
    // Store in ~/.tx3/trix/fingerprint
    if let Ok(mut path) = crate::home::tx3_dir() {
        path.push("trix");
        if std::fs::create_dir_all(&path).is_ok() {
            path.push("fingerprint");
            let _ = std::fs::write(path, fingerprint);
        }
    }
}
