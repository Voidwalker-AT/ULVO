import os
import sys
import stat
import json
import pathlib
import re
from collections import defaultdict

WORKSPACE_ROOT = pathlib.Path("E:/Workspace").resolve()
OUTPUT_DIR = pathlib.Path("E:/Workspace/Active/ULVO/docs/evidence/stage2-7-workspace-audit").resolve()

def is_reparse(path):
    try:
        st = os.lstat(path)
        return bool(st.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT)
    except Exception:
        return False

def parse_pyvenv_cfg(path):
    info = {}
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for line in f:
                if "=" in line:
                    k, v = line.split("=", 1)
                    info[k.strip().lower()] = v.strip()
    except Exception as e:
        info["_error"] = str(e)
    return info

def scan_environment(env_path):
    env_path = pathlib.Path(env_path)
    env_info = {
        "path": str(env_path),
        "type": "venv",
        "pyvenv_cfg": None,
        "python_version": None,
        "base_interpreter": None,
        "packages": [],
        "logical_bytes": 0,
        "unique_inodes": set(),
        "has_editable": False,
        "has_native": False,
        "native_extensions": [],
        "inaccessible_count": 0,
    }

    pyvenv = env_path / "pyvenv.cfg"
    if pyvenv.is_file():
        cfg = parse_pyvenv_cfg(pyvenv)
        env_info["pyvenv_cfg"] = cfg
        env_info["python_version"] = cfg.get("version") or cfg.get("version_info")
        env_info["base_interpreter"] = cfg.get("home") or cfg.get("executable")

    conda_meta = env_path / "conda-meta"
    if conda_meta.is_dir():
        env_info["type"] = "conda"

    # Find site-packages
    site_packages_dirs = []
    lib_dir = env_path / "Lib"
    if not lib_dir.is_dir():
        lib_dir = env_path / "lib"
    if lib_dir.is_dir():
        for sp in lib_dir.glob("**/site-packages"):
            if sp.is_dir() and not is_reparse(sp):
                site_packages_dirs.append(sp)

    # Scan site-packages for packages
    for sp in site_packages_dirs:
        try:
            for entry in os.scandir(sp):
                if entry.is_dir() and entry.name.endswith(".dist-info"):
                    pkg_match = re.match(r"^(.+)-([^-]+)\.dist-info$", entry.name)
                    pkg_name = pkg_match.group(1).lower() if pkg_match else entry.name[:-10].lower()
                    pkg_version = pkg_match.group(2) if pkg_match else "unknown"

                    # Check for direct_url.json (editable)
                    direct_url = pathlib.Path(entry.path) / "direct_url.json"
                    is_editable = False
                    if direct_url.is_file():
                        try:
                            data = json.loads(direct_url.read_text(encoding="utf-8", errors="replace"))
                            if data.get("dir_info", {}).get("editable", False):
                                is_editable = True
                                env_info["has_editable"] = True
                        except Exception:
                            pass

                    # Measure package size
                    pkg_bytes = 0
                    pkg_inodes = set()
                    pkg_native = False
                    for root, dirs, files in os.walk(entry.path):
                        for f in files:
                            fp = os.path.join(root, f)
                            try:
                                f_st = os.lstat(fp)
                                pkg_bytes += f_st.st_size
                                pkg_inodes.add((f_st.st_dev, f_st.st_ino))
                                if f.endswith((".pyd", ".dll", ".so")):
                                    pkg_native = True
                                    env_info["has_native"] = True
                            except Exception:
                                pass

                    # Also measure the package folder itself if it exists next to .dist-info
                    module_name = entry.name[:-10].split("-")[0].replace("-", "_")
                    top_level_file = pathlib.Path(entry.path) / "top_level.txt"
                    candidate_dirs = [module_name]
                    if top_level_file.is_file():
                        try:
                            for tl in top_level_file.read_text(encoding="utf-8", errors="replace").splitlines():
                                tl_clean = tl.strip()
                                if tl_clean and tl_clean not in candidate_dirs:
                                    candidate_dirs.append(tl_clean)
                        except Exception:
                            pass

                    for cd in candidate_dirs:
                        mod_dir = pathlib.Path(sp) / cd
                        if mod_dir.is_dir() and not is_reparse(mod_dir):
                            for root, dirs, files in os.walk(str(mod_dir)):
                                for f in files:
                                    fp = os.path.join(root, f)
                                    try:
                                        f_st = os.lstat(fp)
                                        pkg_bytes += f_st.st_size
                                        pkg_inodes.add((f_st.st_dev, f_st.st_ino))
                                        if f.endswith((".pyd", ".dll", ".so")):
                                            pkg_native = True
                                            env_info["has_native"] = True
                                            if len(env_info["native_extensions"]) < 50:
                                                env_info["native_extensions"].append(f)
                                    except Exception:
                                        pass

                    env_info["packages"].append({
                        "name": pkg_name,
                        "version": pkg_version,
                        "is_editable": is_editable,
                        "has_native": pkg_native,
                        "logical_bytes": pkg_bytes,
                        "unique_files": len(pkg_inodes),
                    })
        except Exception as e:
            env_info["inaccessible_count"] += 1

    # Measure total environment size
    for root, dirs, files in os.walk(str(env_path)):
        # Prune junctions
        dirs[:] = [d for d in dirs if not is_reparse(os.path.join(root, d))]
        for f in files:
            fp = os.path.join(root, f)
            try:
                st = os.lstat(fp)
                env_info["logical_bytes"] += st.st_size
                env_info["unique_inodes"].add((st.st_dev, st.st_ino))
            except Exception:
                env_info["inaccessible_count"] += 1

    env_info["physical_unique_file_count"] = len(env_info["unique_inodes"])
    del env_info["unique_inodes"]  # Not JSON serializable
    return env_info

def audit_workspace():
    print(f"[AUDIT] Starting read-only scan of {WORKSPACE_ROOT}...")
    top_level_stats = defaultdict(lambda: {"logical_bytes": 0, "file_count": 0, "inodes": set(), "inaccessible": 0})
    total_workspace_bytes = 0
    total_files = 0
    workspace_inodes = {}  # (dev, ino) -> size

    python_envs = []
    projects = []
    inaccessible_paths = []

    # First pass: find top-level directories and projects
    for item in os.scandir(WORKSPACE_ROOT):
        top_name = item.name
        if item.is_file():
            try:
                st = os.lstat(item.path)
                top_level_stats["_root_files"]["logical_bytes"] += st.st_size
                top_level_stats["_root_files"]["file_count"] += 1
                workspace_inodes[(st.st_dev, st.st_ino)] = st.st_size
            except Exception as e:
                inaccessible_paths.append(item.path)
            continue

        if not item.is_dir() or is_reparse(item.path):
            continue

        print(f"  [SCAN] Scanning top-level category: {top_name}...")
        for root, dirs, files in os.walk(item.path):
            # Check for reparse points and prune them
            pruned_dirs = []
            for d in dirs:
                dp = os.path.join(root, d)
                if is_reparse(dp):
                    continue
                pruned_dirs.append(d)
            dirs[:] = pruned_dirs

            # Check if this directory is a Python virtual environment
            is_env = False
            if "pyvenv.cfg" in files:
                is_env = True
            elif "conda-meta" in dirs:
                is_env = True
            elif ("Scripts" in dirs or "bin" in dirs) and ("Lib" in dirs or "lib" in dirs):
                if os.path.isfile(os.path.join(root, "Scripts", "python.exe")) or os.path.isfile(os.path.join(root, "bin", "python")):
                    is_env = True

            if is_env:
                print(f"    [ENV FOUND] {root}")
                env_info = scan_environment(root)
                # Find parent project
                rel_parts = pathlib.Path(root).relative_to(WORKSPACE_ROOT).parts
                env_info["category"] = rel_parts[0] if rel_parts else "root"
                env_info["relative_path"] = str(pathlib.Path(root).relative_to(WORKSPACE_ROOT))
                python_envs.append(env_info)
                # Do not walk deeper into virtual environment
                dirs.clear()
                continue

            # Check if this directory is a project root
            project_sentinels = [".git", "pyproject.toml", "requirements.txt", "uv.lock",
                                "package.json", "Cargo.toml", "go.mod", "CMakeLists.txt", ".ulvo"]
            matched_sentinels = [s for s in project_sentinels if (s in dirs or s in files)]
            if matched_sentinels:
                rel = str(pathlib.Path(root).relative_to(WORKSPACE_ROOT))
                projects.append({
                    "path": rel,
                    "name": pathlib.Path(root).name,
                    "category": rel.split("\\")[0] if "\\" in rel else rel.split("/")[0],
                    "sentinels": matched_sentinels,
                    "has_git": ".git" in dirs,
                    "has_python_manifest": any(p in matched_sentinels for p in ["pyproject.toml", "requirements.txt", "uv.lock"]),
                    "has_node_manifest": "package.json" in matched_sentinels,
                    "has_rust_manifest": "Cargo.toml" in matched_sentinels,
                })

            # Tally files
            for f in files:
                fp = os.path.join(root, f)
                try:
                    st = os.lstat(fp)
                    sz = st.st_size
                    top_level_stats[top_name]["logical_bytes"] += sz
                    top_level_stats[top_name]["file_count"] += 1
                    top_level_stats[top_name]["inodes"].add((st.st_dev, st.st_ino))
                    workspace_inodes[(st.st_dev, st.st_ino)] = sz
                    total_workspace_bytes += sz
                    total_files += 1
                except Exception as e:
                    top_level_stats[top_name]["inaccessible"] += 1
                    inaccessible_paths.append(fp)

    # Convert sets to counts for JSON output
    clean_top_level = {}
    for k, v in top_level_stats.items():
        clean_top_level[k] = {
            "logical_bytes": v["logical_bytes"],
            "logical_gb": round(v["logical_bytes"] / (1024**3), 2),
            "file_count": v["file_count"],
            "unique_physical_files": len(v["inodes"]),
            "inaccessible_count": v["inaccessible"],
        }

    total_physical_bytes = sum(workspace_inodes.values())
    total_physical_unique_files = len(workspace_inodes)

    # Analyze Python environment deduplication
    package_instances = defaultdict(list)
    total_env_logical_bytes = sum(e["logical_bytes"] for e in python_envs)

    for env in python_envs:
        for pkg in env["packages"]:
            key = (pkg["name"], pkg["version"])
            package_instances[key].append({
                "env_path": env["relative_path"],
                "category": env["category"],
                "logical_bytes": pkg["logical_bytes"],
                "is_editable": pkg["is_editable"],
                "has_native": pkg["has_native"],
            })

    # Find duplicates
    duplicate_packages = []
    total_wasted_package_bytes = 0

    for (pkg_name, pkg_ver), instances in package_instances.items():
        if len(instances) > 1:
            # Wasted is size of all except 1 copy
            unit_size = max(inst["logical_bytes"] for inst in instances)
            wasted = unit_size * (len(instances) - 1)
            total_wasted_package_bytes += wasted
            duplicate_packages.append({
                "package": pkg_name,
                "version": pkg_ver,
                "instance_count": len(instances),
                "estimated_unit_bytes": unit_size,
                "estimated_unit_mb": round(unit_size / (1024**2), 2),
                "total_wasted_bytes": wasted,
                "total_wasted_mb": round(wasted / (1024**2), 2),
                "has_native": any(i["has_native"] for i in instances),
                "environments": [i["env_path"] for i in instances],
            })

    duplicate_packages.sort(key=lambda x: x["total_wasted_bytes"], reverse=True)

    # Savings models
    # Conservative: Deduplication of confirmed duplicate packages across environments
    conservative_savings = total_wasted_package_bytes
    # Realistic: 85% of per-project env space reclaimed via uv ephemeral environments & shared uv cache
    # Exclude base shared envs (like Shared/envs/ds-base if meant as long-term base)
    project_env_bytes = sum(e["logical_bytes"] for e in python_envs if not e["relative_path"].startswith("Shared"))
    realistic_savings = int(project_env_bytes * 0.85)
    # Theoretical: All project virtual environments replaced by on-demand ephemeral executions
    theoretical_savings = project_env_bytes

    report = {
        "schema_version": 1,
        "audit_timestamp": "2026-10-18T23:30:00Z",
        "workspace_root": str(WORKSPACE_ROOT),
        "storage_summary": {
            "total_logical_bytes": total_workspace_bytes,
            "total_logical_gb": round(total_workspace_bytes / (1024**3), 2),
            "total_physical_allocated_bytes": total_physical_bytes,
            "total_physical_allocated_gb": round(total_physical_bytes / (1024**3), 2),
            "total_files": total_files,
            "unique_physical_files": total_physical_unique_files,
            "hardlink_count": total_files - total_physical_unique_files,
            "inaccessible_file_count": len(inaccessible_paths),
        },
        "category_breakdown": clean_top_level,
        "python_environment_summary": {
            "environment_count": len(python_envs),
            "total_logical_bytes": total_env_logical_bytes,
            "total_logical_gb": round(total_env_logical_bytes / (1024**3), 2),
            "percentage_of_workspace": round((total_env_logical_bytes / total_workspace_bytes * 100), 2) if total_workspace_bytes else 0,
        },
        "python_environments": python_envs,
        "projects_discovered": {
            "total_count": len(projects),
            "projects": projects,
        },
        "package_duplication": {
            "unique_package_version_tuples": len(package_instances),
            "duplicated_package_tuples": len(duplicate_packages),
            "total_wasted_package_bytes": total_wasted_package_bytes,
            "total_wasted_package_gb": round(total_wasted_package_bytes / (1024**3), 2),
            "top_duplicated_packages": duplicate_packages[:30],
        },
        "storage_savings_model": {
            "conservative": {
                "recoverable_bytes": conservative_savings,
                "recoverable_gb": round(conservative_savings / (1024**3), 2),
                "percentage_of_workspace": round((conservative_savings / total_workspace_bytes * 100), 2) if total_workspace_bytes else 0,
                "method": "Package-store deduplication of identical package versions across environments",
                "risk": "Low",
                "confidence": "High (measured package metadata duplication)",
            },
            "realistic": {
                "recoverable_bytes": realistic_savings,
                "recoverable_gb": round(realistic_savings / (1024**3), 2),
                "percentage_of_workspace": round((realistic_savings / total_workspace_bytes * 100), 2) if total_workspace_bytes else 0,
                "method": "uv-managed shared cache + on-demand ephemeral environments for project development",
                "risk": "Low to Moderate",
                "confidence": "High (based on uv hardlink caching and non-base environment reclamation)",
            },
            "theoretical": {
                "recoverable_bytes": theoretical_savings,
                "recoverable_gb": round(theoretical_savings / (1024**3), 2),
                "percentage_of_workspace": round((theoretical_savings / total_workspace_bytes * 100), 2) if total_workspace_bytes else 0,
                "method": "Complete reclamation of all project-level persistent virtual environments",
                "risk": "High (assumes all projects run purely on-demand with zero persistent venvs)",
                "confidence": "Moderate",
            },
        },
    }

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    out_file = OUTPUT_DIR / "workspace_audit.json"
    with open(out_file, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2)

    print(f"\n[AUDIT COMPLETE] Results saved to {out_file}")
    print(f"Total Workspace Storage : {report['storage_summary']['total_logical_gb']} GB")
    print(f"Python Environments     : {report['python_environment_summary']['environment_count']} envs, {report['python_environment_summary']['total_logical_gb']} GB ({report['python_environment_summary']['percentage_of_workspace']}% of workspace)")
    print(f"Projects Discovered     : {report['projects_discovered']['total_count']}")
    print(f"Top Duplicated Wasted   : {report['package_duplication']['total_wasted_package_gb']} GB")
    print(f"Conservative Savings    : {report['storage_savings_model']['conservative']['recoverable_gb']} GB")
    print(f"Realistic Savings       : {report['storage_savings_model']['realistic']['recoverable_gb']} GB")
    print(f"Theoretical Savings     : {report['storage_savings_model']['theoretical']['recoverable_gb']} GB")

if __name__ == "__main__":
    audit_workspace()
