use anyhow::Result;

const FISH_WRAPPER: &str = r#"function wt
    switch $argv[1]
        case switch
            set -l workspace_path (command jjwt switch $argv[2..])
            or return $status
            if test -n "$workspace_path" -a -d "$workspace_path"
                cd "$workspace_path"
            end
        case '*'
            command jjwt $argv
    end
end
"#;

pub fn run_fish() -> Result<()> {
    print!("{FISH_WRAPPER}");

    Ok(())
}
