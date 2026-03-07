use std::fs;
use std::path::Path;

const SKILLS: &[(&str, &[&str])] = &[
    (
        "valid-check",
        &[
            "name: valid-check",
            "description:",
            "disable-model-invocation: true",
            "MCP",
            "cargo valid",
            "cargo run -q --bin valid --",
        ],
    ),
    (
        "valid-testgen",
        &[
            "name: valid-testgen",
            "description:",
            "disable-model-invocation: true",
            "MCP",
            "generate-tests",
            "coverage",
        ],
    ),
    (
        "valid-model",
        &[
            "name: valid-model",
            "description:",
            "disable-model-invocation: true",
            "Rust-first",
            "inspect",
            "verify",
        ],
    ),
    (
        "valid-review",
        &[
            "name: valid-review",
            "description:",
            "disable-model-invocation: true",
            "capability matrix",
            "coverage",
            "solver",
        ],
    ),
    (
        "valid-contract",
        &[
            "name: valid-contract",
            "description:",
            "disable-model-invocation: true",
            "contract",
            "check",
            "drift",
        ],
    ),
];

#[test]
fn claude_skill_files_exist_with_required_metadata() {
    for (skill, required_snippets) in SKILLS {
        let skill_path = Path::new(".claude/skills").join(skill).join("SKILL.md");
        assert!(
            skill_path.is_file(),
            "expected skill definition at {}",
            skill_path.display()
        );

        let body = fs::read_to_string(&skill_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", skill_path.display()));

        assert!(
            body.starts_with("---\n"),
            "{} should start with YAML frontmatter",
            skill_path.display()
        );
        assert!(
            body.contains("\n---\n\n"),
            "{} should close YAML frontmatter before the markdown body",
            skill_path.display()
        );

        for snippet in *required_snippets {
            assert!(
                body.contains(snippet),
                "{} should contain `{snippet}`",
                skill_path.display()
            );
        }
    }
}
