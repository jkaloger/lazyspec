use std::fs;
use std::path::Path;

pub fn render_template(template_content: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template_content.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}

pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn next_number(dir: &Path, prefix: &str) -> u32 {
    let mut max = 0u32;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix) {
                if let Some(rest) = name.strip_prefix(prefix) {
                    let rest = rest.trim_start_matches('-');
                    let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(n) = num_str.parse::<u32>() {
                        max = max.max(n);
                    }
                }
            }
        }
    }
    max + 1
}

pub fn resolve_filename(pattern: &str, doc_type: &str, title: &str, dir: &Path) -> String {
    let slug = slugify(title);
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let type_upper = doc_type.to_uppercase();
    let n = next_number(dir, &type_upper);

    let mut filename = pattern.to_string();
    filename = filename.replace("{type}", &type_upper);
    filename = filename.replace("{title}", &slug);
    filename = filename.replace("{date}", &date);

    if filename.contains("{n:03}") {
        filename = filename.replace("{n:03}", &format!("{:03}", n));
    } else if filename.contains("{n}") {
        filename = filename.replace("{n}", &n.to_string());
    }

    filename
}
