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

use arboard::Clipboard;
use clap::Parser;
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

fn main() {
    let args = Args::parse();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(args.verbosity))
        .init();
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

fn strip_full_youtube(url: &url::Url) -> url::Url {
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
        let url1 =
            Url::parse("https://www.youtube.com/watch?v=1234&si=stripped&feature=share").unwrap();
        let stripped_url1 = strip_tracking(&url1);
        assert_eq!(
            stripped_url1.as_str(),
            "https://youtu.be/1234?feature=share",
            "Stripped URL is incorrect"
        );

        let url2 =
            Url::parse("https://music.youtube.com/watch?v=5678&si=stripped&feature=share").unwrap();
        let stripped_url2 = strip_tracking(&url2);
        assert_eq!(
            stripped_url2.as_str(),
            "https://music.youtube.com/watch?v=5678&feature=share",
            "Stripped URL is incorrect"
        );

        let url3 = Url::parse("https://example.com/path?utm_source=foo&utm_medium=bar").unwrap();
        let stripped_url3 = strip_tracking(&url3);
        assert_eq!(
            stripped_url3.as_str(),
            "https://example.com/path",
            "Stripped URL is incorrect"
        );

        let url4: Url = Url::parse("https://youtu.be/1234?si=stripped&t=123").unwrap();
        let stripped_url4 = strip_tracking(&url4);
        assert_eq!(
            stripped_url4.as_str(),
            "https://youtu.be/1234?t=123",
            "Stripped URL is incorrect"
        );
    }
}
