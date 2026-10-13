# LightMem Testing Process & Integrity Standard

## 1. Dual-Test Architecture

Every module and public/internal function in **LightMem** is verified through two distinct lenses inside `tests/`:

1. **Functionality Test (`func_*`)**: Verifies the expected contract, core happy-path behavior, output shape, and feature completeness of the function.
2. **Integrity Test (`integrity_*`)**: Verifies invariants, boundary conditions, malformed input rejection, cryptographic/hash verification, transactional rollback on failure, concurrency/revision safety, and resistance to data corruption or injection.

---

## 2. Layer-by-Layer Test Suites (`tests/`)

| Layer | Source Module(s) | Test Suite (`tests/*.rs`) | Documentation Matrix (`tests/documentation/*.md`) |
| :--- | :--- | :--- | :--- |
| **Layer 1: Models** | `src/models/{config,memory,link,stats}.rs` | `tests/func_integrity_models.rs` | `01_models_test_matrix.md` |
| **Layer 2: Repository** | `src/repositories/sqlite_repo.rs` | `tests/func_integrity_repository.rs` | `02_repository_test_matrix.md` |
| **Layer 3: Services** | `src/services/{embeddings,search,reranker,observer,exporter,importer,connect,graph_view}.rs` | `tests/func_integrity_services.rs` | `03_services_test_matrix.md` |
| **Layer 4: Controller** | `src/controllers/memory_controller.rs` | `tests/func_integrity_controller.rs` | `04_controller_test_matrix.md` |
| **Layer 5: CLI & View** | `src/views/cli_view.rs`, `src/main.rs` | `tests/func_integrity_cli_and_view.rs` | `05_cli_and_view_test_matrix.md` |
| **Layer 6: Python SDK** | `python/src/lmem/{client,models,binary,cli}.py` | `python/tests/test_sdk.py` | `06_python_sdk_test_matrix.md` |

---

## 3. Running the Suites

```bash
# Run all Rust functionality & integrity suites
cargo test --test func_integrity_models
cargo test --test func_integrity_repository
cargo test --test func_integrity_services
cargo test --test func_integrity_controller
cargo test --test func_integrity_cli_and_view

# Run the entire workspace test suite (including legacy integration & regression tests)
cargo test

# Run Python SDK functionality & integrity tests
pytest python/tests/test_sdk.py
```
