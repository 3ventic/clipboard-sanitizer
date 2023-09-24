/*
Clipboard Sanitizer - A simple program to strip tracking parameters from URLs in the clipboard
Copyright (C) 2023  Werner Vänttinen

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use std::collections::HashMap;

use arboard::Clipboard;
use clap::Parser;
use config::Config;
use dirs::config_dir;
use log::{debug, error, info};
use url::Url;

const YOUTUBE_TRACKING_PARAMS: [&str; 1] = ["si"];
const COMMON_TRACKING_PARAMS: [&str; 5] = [
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
];
const DEFAULT_LOG_LEVEL: &str = "info";

#[derive(Parser, Debug)]
#[command(name = "clipboard-sanitizer", version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"), about = env!("CARGO_PKG_DESCRIPTION"))]
struct Args {
    #[arg(short = 'v', long = "verbose", default_value_t = DEFAULT_LOG_LEVEL.to_string())]
    verbosity: String,
}

static mut APP_CONFIG: Option<HashMap<String, String>> = None;

fn main() {
    let args = Args::parse();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(args.verbosity))
        .init();

    init_settings();

    let mut clipboard = Clipboard::new().unwrap();

    loop {
        debug!("Checking clipboard...");
        let url = parse_url(&mut clipboard);
        if let Some(url) = url {
            let stripped_url = strip_tracking(&url);
            if stripped_url != url {
                if let Err(e) = clipboard.set_text(stripped_url.as_str().to_string()) {
                    error!("Failed to set clipboard: {}", e);
                }
                info!("Stripped tracking from URL: {}", stripped_url);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn init_settings() {
    // Use of unsafe in this function is justified because we are writing the config once before reading it

    let mut config_path = config_dir().unwrap();
    config_path.push("clipboard-sanitizer");

    let default_config: HashMap<String, String> = HashMap::new();

    // path doesn't exist
    if config_path.is_dir() == false {
        let res = std::fs::create_dir_all(&config_path);
        if let Err(e) = res {
            error!(
                "Failed to create config directory {:?}: {:?}",
                config_path, e
            );
            unsafe { APP_CONFIG = Some(default_config) };
            return;
        }
        let res = std::fs::write(config_path.join("config.toml"), "");
        if let Err(e) = res {
            error!(
                "Failed to create config file {:?}: {:?}",
                config_path.join("config.toml"),
                e
            );
            unsafe { APP_CONFIG = Some(default_config) };
            return;
        }
    }

    let cfg = Config::builder()
        .add_source(config::File::from(config_path.join("config.toml")))
        .add_source(config::Environment::with_prefix("CLIPBOARD_SANITIZER"))
        .build()
        .unwrap();

    let map = cfg.try_deserialize::<HashMap<String, String>>().unwrap();
    info!("Using config: {:?}", map);

    unsafe {
        APP_CONFIG = Some(map);
    };
}

fn read_setting(key: &str) -> Option<String> {
    // This is safe because we are only reading
    unsafe {
        if let Some(map) = &APP_CONFIG {
            if let Some(value) = map.get(key) {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// parse_url guarantees that clipboard contains a URL with a domain name and returns the URL instance
fn parse_url(clipboard: &mut Clipboard) -> Option<url::Url> {
    let content = clipboard.get_text();
    if let Ok(potential_url) = content {
        if let Ok(url) = Url::parse(&potential_url) {
            debug!("Found URL: {}", url);
            if let Some(_) = url.domain() {
                return Some(url);
            }
        } else {
            debug!("Clipboard content is not a URL: {}", potential_url);
        }
    } else {
        error!("Failed to get clipboard: {}", content.unwrap_err());
    }
    None
}

fn strip_tracking(url: &url::Url) -> url::Url {
    match url.domain().unwrap() {
        "www.youtube.com" => strip_full_youtube(&url),
        "youtube.com" => strip_full_youtube(&url),
        "youtu.be" => strip_params(&url, YOUTUBE_TRACKING_PARAMS.to_vec()),
        "music.youtube.com" => strip_params(&url, YOUTUBE_TRACKING_PARAMS.to_vec()),
        _ => strip_params(&url, COMMON_TRACKING_PARAMS.to_vec()),
    }
}

fn enabled_prefixes() -> Vec<String> {
    let mut prefixes = vec![];
    if let Some(prefixes_csv) = read_setting("YOUTUBE_PREFIXES") {
        if prefixes_csv == "" {
            return prefixes;
        }
        for prefix in prefixes_csv.split(',') {
            prefixes.push(format!("/{}/", prefix.to_string()));
        }
    }
    prefixes
}

fn map_youtube_prefix(url: &url::Url, prefix: &str) -> Option<url::Url> {
    if url.path().starts_with(prefix) {
        let mut new_url = url.clone();
        new_url.set_host(Some("youtu.be")).unwrap();
        let video_id = url
            .path()
            .strip_prefix(prefix)
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        new_url.set_path(&format!("/{}", video_id));
        return Some(new_url);
    }
    None
}

fn strip_full_youtube(url: &url::Url) -> url::Url {
    let prefixes = enabled_prefixes();
    for prefix in prefixes {
        if let Some(new_url) = map_youtube_prefix(&url, &prefix) {
            return strip_tracking(&new_url);
        }
    }

    if let Some(video_id) = get_query_value(url, "v") {
        let mut new_url = url.clone();
        new_url.set_host(Some("youtu.be")).unwrap();
        new_url.set_path(&format!("/{}", video_id));
        let mut params = YOUTUBE_TRACKING_PARAMS.to_vec();
        params.push("v");
        let new_url = strip_params(&new_url, params);
        new_url
    } else {
        let new_url = strip_params(&url, YOUTUBE_TRACKING_PARAMS.to_vec());
        new_url
    }
}

fn strip_params(url: &url::Url, strip: Vec<&str>) -> url::Url {
    debug!("Stripping params from url {}: {:?}", url, strip);
    let mut query_pairs = url.query_pairs();
    let mut new_url = url.clone();
    new_url.query_pairs_mut().clear();
    while let Some(pair) = query_pairs.next() {
        if !strip.contains(&pair.0.as_ref()) {
            new_url.query_pairs_mut().append_pair(&pair.0, &pair.1);
        }
    }
    if new_url.query_pairs().count() == 0 {
        new_url.set_query(None);
    }
    new_url
}

fn get_query_value(url: &url::Url, var: &str) -> Option<String> {
    for (key, value) in url.query_pairs() {
        if key == var {
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_test_settings() {
        let mut app_config = HashMap::new();
        app_config.insert("YOUTUBE_PREFIXES".to_string(), "live,shorts".to_string());
        unsafe {
            APP_CONFIG = Some(app_config);
        }
    }

    #[test]
    fn test_strip_params() {
        let url = Url::parse("https://example.com/path?foo=bar&baz=qux").unwrap();
        let stripped_url = strip_params(&url, vec!["baz"]);
        assert_eq!(
            stripped_url.as_str(),
            "https://example.com/path?foo=bar",
            "Stripped URL is incorrect"
        );
    }

    #[test]
    fn test_strip_tracking() {
        init_test_settings();

        let test_cases = vec![
            (
                "https://www.youtube.com/watch?v=1234&si=stripped&feature=share",
                "https://youtu.be/1234?feature=share",
            ),
            (
                "https://music.youtube.com/watch?v=5678&si=stripped&feature=share",
                "https://music.youtube.com/watch?v=5678&feature=share",
            ),
            (
                "https://example.com/path?utm_source=foo&utm_medium=bar",
                "https://example.com/path",
            ),
            (
                "https://youtu.be/1234?si=stripped&t=123",
                "https://youtu.be/1234?t=123",
            ),
            (
                "https://youtube.com/live/xxxxxxxxxx?feature=share",
                "https://youtu.be/xxxxxxxxxx?feature=share",
            ),
            (
                "https://youtube.com/shorts/xxxxxxxxxx?feature=share",
                "https://youtu.be/xxxxxxxxxx?feature=share",
            ),
        ];

        for (input, expected) in test_cases {
            let url = Url::parse(input).unwrap();
            let stripped_url = strip_tracking(&url);
            assert_eq!(stripped_url.as_str(), expected, "Stripped URL is incorrect");
        }
    }
}
