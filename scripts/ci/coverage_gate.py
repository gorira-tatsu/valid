#!/usr/bin/env python3

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ARTIFACT_DIR = ROOT / "artifacts" / "ci" / "coverage"


def escape_workflow_command(value: str) -> str:
    return value.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def run_command(args: list[str]) -> str:
    completed = subprocess.run(
        args,
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        if completed.stdout:
            print(completed.stdout, file=sys.stderr, end="")
        if completed.stderr:
            print(completed.stderr, file=sys.stderr, end="")
        raise SystemExit(completed.returncode)
    return completed.stdout


def load_models() -> list[str]:
    config_path = ROOT / "valid.toml"
    if config_path.exists():
        config_body = config_path.read_text(encoding="utf-8")
        match = re.search(r"^suite_models\s*=\s*\[(.*?)\]", config_body, re.MULTILINE | re.DOTALL)
        suite_models = []
        if match:
            for raw_item in match.group(1).split(","):
                model = raw_item.strip().strip('"').strip("'")
                if model:
                    suite_models.append(model)
        if suite_models:
            return suite_models

    stdout = run_command(
        ["cargo", "run", "--quiet", "--bin", "cargo-valid", "--", "models", "--json"]
    )
    payload = json.loads(stdout)
    models = payload.get("models", [])
    if not isinstance(models, list) or not models:
        raise SystemExit("coverage gate requires at least one model")
    return [str(model) for model in models]


def main() -> int:
    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)
    models = load_models()

    for model in models:
        stdout = run_command(
            [
                "cargo",
                "run",
                "--quiet",
                "--bin",
                "cargo-valid",
                "--",
                "coverage",
                model,
                "--json",
            ]
        )
        report = json.loads(stdout)
        artifact_path = ARTIFACT_DIR / f"{model}.json"
        artifact_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

        summary = report.get("summary", {})
        gate = report.get("gate", {})
        status = str(gate.get("status", "unknown"))
        transition = summary.get("transition_coverage_percent", "n/a")
        guard = summary.get("guard_full_coverage_percent", "n/a")
        reasons = gate.get("reasons") or []

        print(
            f"coverage[{model}] status={status} "
            f"transition={transition}% guard_full={guard}%"
        )
        if status != "pass":
            reason_text = ", ".join(str(reason) for reason in reasons) or "threshold not met"
            message = (
                f"model={model} transition={transition}% "
                f"guard_full={guard}% reasons={reason_text}"
            )
            print(f"::warning title=Coverage gate::{escape_workflow_command(message)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
