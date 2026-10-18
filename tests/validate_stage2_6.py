import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import sys

from repo_guard import repository_root, guarded_path, fresh_directory, write_new

ROOT = repository_root(__file__)

def hash_files(base_dir):
    p = pathlib.Path(base_dir)
    if not p.is_absolute():
        p = ROOT / p
    res = {}
    if p.is_dir():
        for f in p.rglob('*'):
            if f.is_file():
                rel = str(f.relative_to(ROOT))
                res[rel] = hashlib.sha256(f.read_bytes()).hexdigest()
    return res

def run_cmd(binary, args, cwd=ROOT):
    cmd = [str(binary), *args]
    proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=30)
    return proc.returncode, proc.stdout, proc.stderr

def main():
    parser = argparse.ArgumentParser(description="Stage 2.6 Vault Registry Independent Validation")
    parser.add_argument("--output", default="docs/evidence/stage2-6-vault-registry", help="Output evidence directory")
    args = parser.parse_args()

    binary_path = guarded_path(ROOT, "target/x86_64-pc-windows-gnu/debug/ulvo.exe")
    if not binary_path.exists():
        binary_path = guarded_path(ROOT, "target-local/debug/ulvo.exe")
    if not binary_path.exists():
        binary_path = guarded_path(ROOT, "target-local/debug/ulvo")

    out_dir_path = pathlib.Path(args.output)
    if not out_dir_path.is_absolute():
        out_dir_path = ROOT / out_dir_path
    if not out_dir_path.exists():
        out_dir = fresh_directory(ROOT, args.output)
    else:
        out_dir = guarded_path(ROOT, args.output)

    print(f"[RECON] Validating binary: {binary_path}")
    print(f"[RECON] Evidence directory: {out_dir}")

    # Step 1: Snapshot critical files before tests
    vault_hashes_before = hash_files("vault/runtimes/rust")
    config_hashes_before = hash_files("config")
    fixture_hashes_before = hash_files("tests/fixtures")

    test_results = []
    cases = [
        ("version", ["--version"], 0, False),
        ("help", ["--help"], 0, False),
        ("inventory", ["inventory", "--all-backends", "--json"], 0, True),
        ("inspect-valid", ["inspect", str(ROOT / "tests/fixtures/valid"), "--json"], 0, True),
        ("vault-list", ["vault", "list"], 0, False),
        ("vault-list-json", ["vault", "list", "--json"], 0, True),
        ("vault-stats", ["vault", "stats"], 0, False),
        ("vault-stats-json", ["vault", "stats", "--json"], 0, True),
        ("vault-inspect-rustc", ["vault", "inspect", "rustc"], 0, False),
        ("vault-inspect-rustc-json", ["vault", "inspect", "rustc", "--json"], 0, True),
        ("vault-verify-rustc", ["vault", "verify", "rustc"], 0, False),
        ("vault-verify-rustc-json", ["vault", "verify", "rustc", "--json"], 0, True),
        ("vault-inspect-invalid", ["vault", "inspect", "nonexistent-id"], 1, False),
        ("vault-inspect-invalid-json", ["vault", "inspect", "nonexistent-id", "--json"], 1, True),
        ("vault-verify-invalid", ["vault", "verify", "nonexistent-id"], 1, False),
        ("vault-verify-invalid-json", ["vault", "verify", "nonexistent-id", "--json"], 1, True),
    ]

    for name, cmd_args, expected_code, expect_json in cases:
        code, stdout, stderr = run_cmd(binary_path, cmd_args)
        case_info = {
            "name": name,
            "args": cmd_args,
            "expected_code": expected_code,
            "actual_code": code,
            "passed": (code == expected_code),
            "stdout_len": len(stdout),
            "stderr_len": len(stderr),
        }

        # Write raw stdout & stderr
        (out_dir / f"{name}.stdout.txt").write_text(stdout, encoding="utf-8")
        (out_dir / f"{name}.stderr.txt").write_text(stderr, encoding="utf-8")

        assert code == expected_code, f"Command {name} failed: expected {expected_code}, got {code}. Stderr: {stderr}"

        if expect_json:
            parsed = json.loads(stdout)
            assert "schema_version" in parsed, f"JSON output for {name} missing schema_version"
            assert parsed["schema_version"] == 1, f"JSON schema_version != 1 for {name}"
            case_info["parsed_schema_version"] = parsed["schema_version"]

            if name == "vault-list-json":
                assert "resources" in parsed and "total_resources" in parsed
                assert parsed["total_resources"] == len(parsed["resources"])
                assert parsed["total_resources"] >= 6, "Expected at least 6 managed resources for Rust toolchain"
                # Check field integrity of every resource
                required_fields = [
                    "resource_id", "resource_kind", "ecosystem", "name", "version",
                    "platform", "architecture", "backend", "abi", "installation_path",
                    "manager", "ownership", "verification_status", "provenance", "capabilities"
                ]
                for r in parsed["resources"]:
                    for f in required_fields:
                        assert f in r, f"Missing required field {f} in resource {r.get('resource_id')}"
                case_info["discovered_resource_count"] = parsed["total_resources"]

            elif name == "vault-stats-json":
                assert "total_resources" in parsed and "by_kind" in parsed and "by_ecosystem" in parsed
                assert parsed["total_resources"] >= 6
                assert parsed["by_ecosystem"].get("rust", 0) >= 6
                case_info["stats_total"] = parsed["total_resources"]

            elif name == "vault-inspect-rustc-json":
                assert "resource" in parsed
                res = parsed["resource"]
                assert res["name"] == "rustc"
                assert res["ecosystem"] == "rust"
                assert res["backend"] == "windows"
                assert res["provenance"] is not None
                assert res["provenance"]["canonical_url"] is not None
                case_info["inspected_resource_id"] = res["resource_id"]

            elif name == "vault-verify-rustc-json":
                assert "resource" in parsed and "output" in parsed
                res = parsed["resource"]
                assert res["verification_status"] == "verified"
                assert res["last_verified_at"] is not None
                assert "1.96.0" in parsed["output"]
                case_info["verified_output"] = parsed["output"]

            elif "invalid-json" in name:
                assert "error" in parsed, f"Expected 'error' in JSON output for {name}"
                case_info["error_message"] = parsed["error"]

        test_results.append(case_info)
        print(f"  [PASS] {name} (exit={code})")

    # Step 2: Test empty vault isolation
    empty_tmp = ROOT / "tests/tmp/empty_vault_validation"
    empty_tmp.mkdir(parents=True, exist_ok=True)
    try:
        code_empty, out_empty, err_empty = run_cmd(binary_path, ["vault", "--vault-root", str(empty_tmp), "list", "--json"])
        assert code_empty == 0
        parsed_empty = json.loads(out_empty)
        assert parsed_empty["total_resources"] == 0
        assert parsed_empty["resources"] == []
        print("  [PASS] empty-vault-isolation (exit=0, total=0)")
        test_results.append({"name": "empty-vault-isolation", "passed": True, "total_resources": 0})
    finally:
        if empty_tmp.exists():
            import shutil
            shutil.rmtree(empty_tmp, ignore_errors=True)

    # Step 3: Verify zero filesystem tampering before vs after
    vault_hashes_after = hash_files("vault/runtimes/rust")
    config_hashes_after = hash_files("config")
    fixture_hashes_after = hash_files("tests/fixtures")

    assert vault_hashes_before == vault_hashes_after, "VAULT FILE MODIFICATION DETECTED!"
    assert config_hashes_before == config_hashes_after, "CONFIG FILE MODIFICATION DETECTED!"
    assert fixture_hashes_before == fixture_hashes_after, "FIXTURE FILE MODIFICATION DETECTED!"
    print("  [PASS] Zero filesystem tampering: all vault, config, and fixture hashes match byte-for-byte.")

    summary = {
        "stage": "2.6",
        "description": "Universal Vault Registry Foundation Validation",
        "binary": str(binary_path),
        "total_checks": len(test_results),
        "passed_checks": sum(1 for t in test_results if t["passed"]),
        "failed_checks": sum(1 for t in test_results if not t["passed"]),
        "vault_files_monitored": len(vault_hashes_before),
        "vault_tampering_detected": False,
        "config_tampering_detected": False,
        "fixtures_tampering_detected": False,
        "tests": test_results,
    }

    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"\n[EVIDENCE] Written validation summary to {out_dir / 'summary.json'}")
    print(f"VERDICT: PASS ({summary['passed_checks']}/{summary['total_checks']} checks passed)")

if __name__ == "__main__":
    main()
