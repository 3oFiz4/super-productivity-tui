use std::io::Write;

/// Dispatches optional OS desktop notification and terminal bell / audio cue
/// without blocking the TUI event loop.
pub fn dispatch(desktop_enabled: bool, sound_enabled: bool, title: &str, body: &str) {
    if cfg!(test) {
        return;
    }

    if sound_enabled {
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x07");
        let _ = out.flush();
        play_os_sound();
    }

    if desktop_enabled {
        send_os_notification(title.to_string(), body.to_string());
    }
}

#[allow(dead_code)]
pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn play_os_sound() {
    std::thread::spawn(|| {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let _ = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-WindowStyle",
                    "Hidden",
                    "-Command",
                    "[System.Media.SystemSounds]::Asterisk.Play(); Start-Sleep -Milliseconds 150",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("afplay")
                .arg("/System/Library/Sounds/Glass.aiff")
                .status();
        }
        #[cfg(target_os = "linux")]
        {
            let ok = std::process::Command::new("canberra-gtk-play")
                .args(["-i", "bell"])
                .status()
                .is_ok_and(|s| s.success());
            if !ok {
                let _ = std::process::Command::new("paplay")
                    .arg("/usr/share/sounds/freedesktop/stereo/complete.oga")
                    .status();
            }
        }
    });
}

fn send_os_notification(title: String, body: String) {
    std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let t = escape_xml(&title);
            let b = escape_xml(&body);
            let script = format!(
                r#"
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml('<toast><visual><binding template="ToastText02"><text id="1">{t}</text><text id="2">{b}</text></binding></visual></toast>')
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Super Productivity TUI').Show($toast)
"#
            );
            let _ = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-WindowStyle",
                    "Hidden",
                    "-Command",
                    &script,
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
        #[cfg(target_os = "macos")]
        {
            let t = title.replace('\\', "\\\\").replace('"', "\\\"");
            let b = body.replace('\\', "\\\\").replace('"', "\\\"");
            let script = format!("display notification \"{b}\" with title \"{t}\"");
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .status();
        }
        #[cfg(target_os = "linux")]
        {
            let _ = std::process::Command::new("notify-send")
                .args(["-a", "Super Productivity TUI", &title, &body])
                .status();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::escape_xml;

    #[test]
    fn escapes_xml_special_characters() {
        assert_eq!(
            escape_xml(r#"A & B <tag> "hi" 'yo'"#),
            "A &amp; B &lt;tag&gt; &quot;hi&quot; &apos;yo&apos;"
        );
    }
}
