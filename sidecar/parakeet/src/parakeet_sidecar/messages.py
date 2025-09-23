"""Pydantic schemas for sidecar command and response payloads."""
from __future__ import annotations

from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field, validator


class LoadModelRequest(BaseModel):
    type: Literal["load_model"] = Field(alias="type")
    model_id: str = Field(..., description="Hugging Face repo id or local identifier")
    local_path: Optional[str] = Field(
        default=None, description="Absolute path to the model directory if already downloaded"
    )
    cache_dir: Optional[str] = Field(
        default=None, description="Directory to use as Hugging Face cache"
    )
    precision: Literal["bf16", "fp32"] = Field(default="bf16")
    attention: Literal["full", "local"] = Field(default="full")
    local_attention_context: int = Field(default=256)
    chunk_duration: Optional[float] = Field(
        default=120.0,
        description="Default chunk duration in seconds for long audio handling",
    )
    overlap_duration: Optional[float] = Field(
        default=15.0, description="Overlap in seconds when chunking audio"
    )
    eager_unload: bool = Field(
        default=False,
        description="If true, unload the currently loaded model even if it's the same id",
    )

    @validator("local_attention_context")
    def _validate_context(cls, value: int, values: Dict[str, Any]) -> int:
        if value <= 0:
            raise ValueError("local_attention_context must be positive")
        return value


class UnloadModelRequest(BaseModel):
    type: Literal["unload_model"] = Field(alias="type")


class TranscribeRequest(BaseModel):
    type: Literal["transcribe"] = Field(alias="type")
    audio_path: str = Field(..., description="Absolute path to the audio file to transcribe")
    language: Optional[str] = Field(default=None)
    translate_to_english: bool = Field(default=False)
    prompt: Optional[str] = Field(default=None)
    use_word_timestamps: bool = Field(default=True)
    chunk_duration: Optional[float] = Field(default=None)
    overlap_duration: Optional[float] = Field(default=None)
    attention: Optional[Literal["full", "local"]] = Field(default=None)
    local_attention_context: Optional[int] = Field(default=None)

    @validator("audio_path")
    def _check_audio_path(cls, value: str) -> str:
        if not value:
            raise ValueError("audio_path cannot be empty")
        return value


class StatusRequest(BaseModel):
    type: Literal["status"] = Field(alias="type")


class ShutdownRequest(BaseModel):
    type: Literal["shutdown"] = Field(alias="type")


CommandRequest = LoadModelRequest | UnloadModelRequest | TranscribeRequest | StatusRequest | ShutdownRequest


class SegmentPayload(BaseModel):
    text: str
    start: Optional[float] = None
    end: Optional[float] = None
    tokens: Optional[List[Dict[str, Any]]] = None


class BaseResponse(BaseModel):
    type: str


class OkResponse(BaseResponse):
    type: Literal["ok"] = "ok"
    command: str
    payload: Dict[str, Any] = Field(default_factory=dict)


class ErrorResponse(BaseResponse):
    type: Literal["error"] = "error"
    code: str = "unknown"
    message: str
    details: Optional[Dict[str, Any]] = None


class StatusResponse(BaseResponse):
    type: Literal["status"] = "status"
    loaded_model: Optional[str] = None
    model_path: Optional[str] = None
    precision: Optional[str] = None
    attention: Optional[str] = None


class TranscriptionResponse(BaseResponse):
    type: Literal["transcription"] = "transcription"
    text: str
    segments: List[SegmentPayload] = Field(default_factory=list)
    language: Optional[str] = None
    duration: Optional[float] = None


Response = OkResponse | ErrorResponse | StatusResponse | TranscriptionResponse


def parse_command(payload: Dict[str, Any]) -> CommandRequest:
    """Dynamically parse an incoming payload into the correct request model."""
    if "type" not in payload:
        raise ValueError("Missing 'type' in message payload")

    msg_type = payload["type"]

    if msg_type == "load_model":
        return LoadModelRequest.model_validate(payload)
    if msg_type == "unload_model":
        return UnloadModelRequest.model_validate(payload)
    if msg_type == "transcribe":
        return TranscribeRequest.model_validate(payload)
    if msg_type == "status":
        return StatusRequest.model_validate(payload)
    if msg_type == "shutdown":
        return ShutdownRequest.model_validate(payload)

    raise ValueError(f"Unknown message type: {msg_type}")
