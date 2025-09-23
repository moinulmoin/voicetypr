"""Entry point for the Parakeet MLX transcription sidecar."""
from __future__ import annotations

import logging
import signal
import sys
from typing import Any, Dict

import orjson
from pydantic import ValidationError

from parakeet_sidecar.messages import (
    CommandRequest,
    ErrorResponse,
    LoadModelRequest,
    OkResponse,
    ShutdownRequest,
    StatusRequest,
    StatusResponse,
    TranscribeRequest,
    TranscriptionResponse,
    UnloadModelRequest,
    parse_command,
)
from parakeet_sidecar.model_manager import ModelManager

LOGGER = logging.getLogger("parakeet_sidecar")


def configure_logging() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s | %(levelname)s | %(name)s | %(message)s",
    )


def write_response(response: Any) -> None:
    try:
        payload = orjson.dumps(response, default=_pydantic_default)
    except Exception as exc:  # pragma: no cover - catastrophic failure
        LOGGER.exception("Failed to serialise response: %s", exc)
        payload = orjson.dumps(
            ErrorResponse(message="Internal serialisation error", code="serialisation_error"),
            default=_pydantic_default,
        )
    sys.stdout.buffer.write(payload + b"\n")
    sys.stdout.flush()


def _pydantic_default(value: Any) -> Any:
    if hasattr(value, "model_dump"):
        return value.model_dump()
    if hasattr(value, "dict"):
        return value.dict()
    raise TypeError(f"Object of type {type(value)} is not JSON serialisable")


def handle_command(manager: ModelManager, command: CommandRequest) -> None:
    try:
        if isinstance(command, LoadModelRequest):
            manager.load(command)
            write_response(
                OkResponse(
                    command="load_model",
                    payload={
                        "model_id": command.model_id,
                        "precision": command.precision,
                        "attention": command.attention,
                    },
                )
            )
            return

        if isinstance(command, UnloadModelRequest):
            manager.unload()
            write_response(OkResponse(command="unload_model"))
            return

        if isinstance(command, TranscribeRequest):
            result = manager.transcribe(command)
            write_response(result)
            return

        if isinstance(command, StatusRequest):
            status = manager.status()
            write_response(StatusResponse(**status))
            return

        if isinstance(command, ShutdownRequest):
            manager.unload()
            write_response(OkResponse(command="shutdown"))
            raise SystemExit(0)

    except FileNotFoundError as exc:
        LOGGER.error("File not found: %s", exc)
        write_response(
            ErrorResponse(
                code="file_not_found",
                message=str(exc),
            )
        )
    except ValidationError as exc:
        LOGGER.error("Validation error: %s", exc)
        write_response(
            ErrorResponse(
                code="validation_error",
                message="Invalid command payload",
                details={"errors": exc.errors()},
            )
        )
    except RuntimeError as exc:
        LOGGER.error("Runtime error: %s", exc)
        write_response(
            ErrorResponse(
                code="runtime_error",
                message=str(exc),
            )
        )
    except Exception as exc:  # pragma: no cover - defensive
        LOGGER.exception("Unhandled exception while processing command")
        write_response(
            ErrorResponse(
                code="internal_error",
                message=str(exc),
            )
        )


def event_loop(manager: ModelManager) -> None:
    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue

        try:
            payload: Dict[str, Any] = orjson.loads(line)
            command = parse_command(payload)
        except ValidationError as exc:
            LOGGER.error("Received invalid payload: %s", exc)
            write_response(
                ErrorResponse(
                    code="validation_error",
                    message="Invalid payload",
                    details={"errors": exc.errors()},
                )
            )
            continue
        except ValueError as exc:
            LOGGER.error("Failed to parse payload: %s", exc)
            write_response(ErrorResponse(code="parse_error", message=str(exc)))
            continue
        except Exception as exc:  # pragma: no cover - defensive path
            LOGGER.exception("Unexpected error decoding JSON")
            write_response(ErrorResponse(code="parse_error", message=str(exc)))
            continue

        handle_command(manager, command)


def _graceful_shutdown(signum: int, _frame: Any) -> None:  # pragma: no cover - signal path
    LOGGER.info("Received signal %s, shutting down", signum)
    raise SystemExit(0)


def run() -> None:
    configure_logging()
    manager = ModelManager()

    # Register signal handlers for graceful shutdown
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, _graceful_shutdown)

    try:
        event_loop(manager)
    except SystemExit:
        pass
    except Exception:  # pragma: no cover - ensure exit
        LOGGER.exception("Fatal error running sidecar loop")
        raise


if __name__ == "__main__":  # pragma: no cover
    run()
