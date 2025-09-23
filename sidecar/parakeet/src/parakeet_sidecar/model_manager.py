"""Model lifecycle management for the Parakeet MLX sidecar."""
from __future__ import annotations

import inspect
import logging
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, Optional

from huggingface_hub import snapshot_download
from parakeet_mlx import from_pretrained

from parakeet_sidecar.messages import (
    LoadModelRequest,
    SegmentPayload,
    TranscribeRequest,
    TranscriptionResponse,
)

LOGGER = logging.getLogger(__name__)


@dataclass
class LoadedModel:
    model_id: str
    model_path: Path
    precision: str
    attention: str
    chunk_duration: float
    overlap_duration: float
    instance: Any
    transcribe_params: set[str]


class ModelManager:
    """Owns Parakeet model instances and routes transcription calls."""

    def __init__(self) -> None:
        self._lock = threading.RLock()
        self._current: Optional[LoadedModel] = None

    # ------------------------------------------------------------------
    def load(self, request: LoadModelRequest) -> LoadedModel:
        """Load a Parakeet model, reusing the current one if possible."""
        with self._lock:
            if (
                self._current
                and not request.eager_unload
                and self._current.model_id == request.model_id
                and (not request.local_path or self._current.model_path == Path(request.local_path))
            ):
                LOGGER.info("Model %s already loaded; reusing existing instance", request.model_id)
                return self._current

            LOGGER.info(
                "Loading Parakeet model id=%s precision=%s attention=%s",
                request.model_id,
                request.precision,
                request.attention,
            )

            model_path = self._resolve_model_path(request)

            # Load from local path
            model = from_pretrained(model_path)

            self._configure_precision(model, request.precision)
            self._configure_attention(model, request.attention, request.local_attention_context)

            params = set(inspect.signature(model.transcribe).parameters.keys())
            LOGGER.debug("Model transcribe signature parameters: %s", sorted(params))

            loaded = LoadedModel(
                model_id=request.model_id,
                model_path=model_path,
                precision=request.precision,
                attention=request.attention,
                chunk_duration=request.chunk_duration or 120.0,
                overlap_duration=request.overlap_duration or 15.0,
                instance=model,
                transcribe_params=params,
            )

            self._current = loaded
            return loaded

    # ------------------------------------------------------------------
    def unload(self) -> None:
        with self._lock:
            if not self._current:
                return
            model_id = self._current.model_id
            try:
                close_fn = getattr(self._current.instance, "close", None)
                if callable(close_fn):
                    close_fn()
            finally:
                self._current = None
                LOGGER.info("Unloaded Parakeet model %s", model_id)

    # ------------------------------------------------------------------
    def status(self) -> Dict[str, Optional[str]]:
        with self._lock:
            if not self._current:
                return {
                    "loaded_model": None,
                    "model_path": None,
                    "precision": None,
                    "attention": None,
                }
            return {
                "loaded_model": self._current.model_id,
                "model_path": str(self._current.model_path),
                "precision": self._current.precision,
                "attention": self._current.attention,
            }

    # ------------------------------------------------------------------
    def transcribe(self, request: TranscribeRequest) -> TranscriptionResponse:
        with self._lock:
            if not self._current:
                raise RuntimeError("No Parakeet model loaded")

            audio_path = Path(request.audio_path)
            if not audio_path.exists():
                raise FileNotFoundError(f"Audio path not found: {audio_path}")

            args = self._build_kwargs(request)
            LOGGER.debug("Transcribing %s with args=%s", audio_path, args)

            result = self._current.instance.transcribe(str(audio_path), **args)
            payload = self._normalise_result(result)
            return TranscriptionResponse(**payload)

    # ------------------------------------------------------------------
    def _resolve_model_path(self, request: LoadModelRequest) -> Path:
        if request.local_path:
            path = Path(request.local_path)
            if not path.exists():
                raise FileNotFoundError(f"local_path does not exist: {path}")
            return path

        cache_dir = Path(request.cache_dir) if request.cache_dir else None
        LOGGER.info("Downloading Parakeet model %s via snapshot_download", request.model_id)
        path_str = snapshot_download(
            repo_id=request.model_id,
            local_dir=cache_dir,
            local_dir_use_symlinks=False,
        )
        return Path(path_str)

    # ------------------------------------------------------------------
    def _configure_precision(self, model: Any, precision: str) -> None:
        method_map = {
            "bf16": ["to_bf16", "to_bfloat16"],
            "fp32": ["to_float32", "to_fp32"],
        }
        for name in method_map.get(precision, []):
            fn = getattr(model, name, None)
            if callable(fn):
                LOGGER.debug("Applying precision method %s", name)
                fn()
                return
        LOGGER.debug("No precision method applied for precision=%s", precision)

    # ------------------------------------------------------------------
    def _configure_attention(self, model: Any, mode: str, context: int) -> None:
        try:
            # parakeet-mlx exposes attention control on the encoder
            enc = getattr(model, "encoder", None)
            setter = getattr(enc, "set_attention_model", None) if enc is not None else None
            if mode == "local" and callable(setter):
                LOGGER.debug("Setting local attention (rel_pos_local_attn, context=%s)", context)
                setter("rel_pos_local_attn", (context, context))
            else:
                # default full attention is typically 'rel_pos'; if not available, do nothing
                if callable(setter):
                    LOGGER.debug("Setting full attention (rel_pos)")
                    setter("rel_pos", None)
        except Exception:
            LOGGER.exception("Failed to configure attention; proceeding with model defaults")

    # ------------------------------------------------------------------
    def _build_kwargs(self, request: TranscribeRequest) -> Dict[str, Any]:
        assert self._current is not None
        params = self._current.transcribe_params
        merged: Dict[str, Any] = {}

        # Defaults from loaded model configuration
        defaults = {
            "chunk_duration": self._current.chunk_duration,
            "overlap_duration": self._current.overlap_duration,
        }

        for key, value in defaults.items():
            if key in params and value is not None:
                merged[key] = value

        overrides = {
            "language": request.language,
            "translate_to_english": request.translate_to_english,
            "prompt": request.prompt,
            "use_word_timestamps": request.use_word_timestamps,
            "chunk_duration": request.chunk_duration,
            "overlap_duration": request.overlap_duration,
        }

        attention_mode = request.attention or self._current.attention
        context = request.local_attention_context or None

        if "attention" in params:
            merged["attention"] = attention_mode
        if context and "local_attention_context" in params:
            merged["local_attention_context"] = context

        for key, value in overrides.items():
            if value is None:
                continue
            if key in params:
                merged[key] = value

        return merged

    # ------------------------------------------------------------------
    def _normalise_result(self, result: Any) -> Dict[str, Any]:
        text = ""
        language = None
        duration = None
        segments = []

        if isinstance(result, dict):
            text = result.get("text") or result.get("transcription") or ""
            language = result.get("language")
            duration = result.get("duration")
            raw_segments = result.get("segments") or result.get("sentences") or []
        else:
            text = getattr(result, "text", "")
            language = getattr(result, "language", None)
            duration = getattr(result, "duration", None)
            raw_segments = getattr(result, "segments", None) or getattr(result, "sentences", [])

        for segment in raw_segments or []:
            if isinstance(segment, dict):
                seg_text = segment.get("text", "")
                start = segment.get("start")
                end = segment.get("end")
                tokens = segment.get("tokens")
            else:
                seg_text = getattr(segment, "text", "")
                start = getattr(segment, "start", None)
                end = getattr(segment, "end", None)
                tokens = getattr(segment, "tokens", None)

            # Process tokens if they exist
            processed_tokens = None
            if tokens:
                if isinstance(tokens, list):
                    # Convert token objects to dictionaries
                    processed_tokens = []
                    for token in tokens:
                        if isinstance(token, dict):
                            processed_tokens.append(token)
                        elif hasattr(token, "__dict__"):
                            # Convert object to dict
                            processed_tokens.append({
                                "text": getattr(token, "text", ""),
                                "start": getattr(token, "start", None),
                                "end": getattr(token, "end", None),
                                "duration": getattr(token, "duration", None),
                            })
                        else:
                            processed_tokens.append({"text": str(token)})

            segments.append(
                SegmentPayload(
                    text=seg_text,
                    start=start,
                    end=end,
                    tokens=processed_tokens,
                ).model_dump()
            )

        return {
            "text": text,
            "segments": segments,
            "language": language,
            "duration": duration,
        }
