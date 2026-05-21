use crate::core::types::ListRow;

pub fn format_list(rows: &[ListRow]) -> String {
    let mut out = String::new();
    let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
    let path_w = rows.iter().map(|r| r.path.display().to_string().len()).max().unwrap_or(0);

    for r in rows {
        let path_str = r.path.display().to_string();

        if r.url.is_empty() {
            out.push_str(&format!("{:<name_w$}  {:<path_w$}\n", r.name, path_str));
        } else {
            out.push_str(&format!("{:<name_w$}  {:<path_w$}  {}\n", r.name, path_str, r.url));
        }
    }

    out
}
