use std::fs;
use std::path::Path;

const REQUIRED_FRONTMATTER_KEYS: &[&str] = &[
    "description:",
    "argument-hint:",
    "disable-model-invocation: true",
];

const SKILLS: &[(&str, &[&str], &[&str])] = &[
    (
        "valid-check",
        &["name: valid-check"],
        &["MCP", "cargo valid", "cargo run -q --bin valid --"],
    ),
    (
        "valid-testgen",
        &["name: valid-testgen"],
        &["MCP", "generate-tests", "coverage"],
    ),
    (
        "valid-model",
        &["name: valid-model"],
        &["Rust-first", "inspect", "verify"],
    ),
    (
        "valid-review",
        &["name: valid-review"],
        &["capability matrix", "coverage", "solver"],
    ),
    (
        "valid-contract",
        &["name: valid-contract"],
        &["contract", "check", "drift"],
    ),
];

#[test]
fn claude_skill_files_exist_with_required_metadata() {
    for (skill, required_frontmatter, required_body_snippets) in SKILLS {
        let skill_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".claude/skills")
            .join(skill)
            .join("SKILL.md");
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

        let frontmatter_end = body.find("\n---\n\n").unwrap_or_else(|| {
            panic!(
                "{} should close YAML frontmatter before the markdown body",
                skill_path.display()
            )
        });
        let frontmatter = &body[..frontmatter_end + "\n---\n".len()];
        let markdown_body = &body[frontmatter_end + "\n---\n\n".len()..];

        for key in REQUIRED_FRONTMATTER_KEYS {
            assert!(
                frontmatter.contains(key),
                "{} frontmatter should contain `{key}`",
                skill_path.display()
            );
        }

        for key in *required_frontmatter {
            assert!(
                frontmatter.contains(key),
                "{} frontmatter should contain `{key}`",
                skill_path.display()
            );
        }

        for snippet in *required_body_snippets {
            assert!(
                markdown_body.contains(snippet),
                "{} markdown body should contain `{snippet}`",
                skill_path.display()
            );
        }
    }
}
