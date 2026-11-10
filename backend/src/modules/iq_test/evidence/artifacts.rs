//! 从回答正文提取文件；思考和工具增量保留在过程记录中，不混入产物。
use super::*;

pub(super) fn write(directory: &Path, attempt: u32, answer: &str) -> Result<Vec<String>> {
    if answer.trim().is_empty() {
        return Ok(Vec::new());
    }
    let trimmed = answer.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    let blocks = if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
        vec![("html", answer.to_owned())]
    } else if lower.starts_with("<svg") {
        vec![("svg", answer.to_owned())]
    } else {
        fenced(answer)
    };
    let blocks = if blocks.is_empty() {
        vec![("md", answer.to_owned())]
    } else {
        blocks
    };
    let multiple = blocks.len() > 1;
    let mut names = Vec::new();
    for (index, (extension, content)) in blocks.into_iter().enumerate() {
        let name = if multiple {
            format!("output.{attempt}.{}.{extension}", index + 1)
        } else {
            format!("output.{attempt}.{extension}")
        };
        let mut writer = FileWriter::create_new(&directory.join(&name))?;
        writer.write_all(content.as_bytes())?;
        writer.sync_all()?;
        names.push(name);
    }
    Ok(names)
}

fn fenced(answer: &str) -> Vec<(&'static str, String)> {
    let mut blocks = Vec::new();
    let mut open: Option<(char, usize, &'static str, String)> = None;
    for line in answer.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'));
        let count = marker.map_or(0, |marker| {
            trimmed.chars().take_while(|c| *c == marker).count()
        });
        if let Some((character, length, extension, content)) = &mut open {
            if marker == Some(*character) && count >= *length && trimmed[count..].trim().is_empty()
            {
                blocks.push((*extension, std::mem::take(content)));
                open = None;
            } else {
                content.push_str(line);
            }
        } else if count >= 3 {
            let language = trimmed[count..].split_whitespace().next().unwrap_or("");
            open = Some((marker.unwrap(), count, extension(language), String::new()));
        }
    }
    if let Some((_, _, extension, content)) = open {
        blocks.push((extension, content));
    }
    blocks
}

fn extension(language: &str) -> &'static str {
    match language.to_ascii_lowercase().as_str() {
        "html" | "htm" => "html",
        "svg" => "svg",
        "css" => "css",
        "javascript" | "js" => "js",
        "typescript" | "ts" => "ts",
        "jsx" => "jsx",
        "tsx" => "tsx",
        "vue" => "vue",
        "json" => "json",
        "xml" => "xml",
        "python" | "py" => "py",
        "rust" | "rs" => "rs",
        "go" => "go",
        "java" => "java",
        "sql" => "sql",
        "yaml" | "yml" => "yaml",
        "markdown" | "md" => "md",
        "bash" | "shell" | "sh" => "sh",
        _ => "txt",
    }
}
