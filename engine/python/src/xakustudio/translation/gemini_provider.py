"""Context-aware dialogue translation provider powered by Google Gemini (google-genai)."""

from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING, Any

from pydantic import BaseModel, Field

from xakustudio.core.errors import (
    MissingCredentialError,
    TaskExecutionError,
    UnsupportedLanguageError,
)
from xakustudio.translation.base import (
    TranslatedItemOutput,
    TranslationItemInput,
    TranslationProvider,
)

if TYPE_CHECKING:
    from xakustudio.worker.server import TaskContext

SUPPORTED_PAIRS: set[tuple[str, str]] = {
    ("zh", "km"),
    ("zh", "en"),
    ("en", "km"),
    ("km", "en"),
}

LANG_NAMES: dict[str, str] = {
    "zh": "Chinese (Mandarin)",
    "km": "Khmer (Cambodian)",
    "en": "English",
}


class _TranslatedLineSchema(BaseModel):
    segment_id: str = Field(description="Exact segment_id from the input batch.")
    translated_text: str = Field(
        description="Natural spoken dialogue translation in the target language."
    )
    notes: str = Field(
        default="",
        description="Optional short localization note (e.g. pronoun/tone choice).",
    )


class _TranslationBatchSchema(BaseModel):
    translations: list[_TranslatedLineSchema]


def build_translation_prompts(
    items: list[TranslationItemInput],
    source_lang: str,
    target_lang: str,
    glossary: list[dict[str, str]],
    scene_context: str,
    previous_context: list[dict[str, str]],
) -> tuple[str, str]:
    """Construct system instruction and user prompt preserving character and glossary context."""
    src_name = LANG_NAMES.get(source_lang, source_lang)
    tgt_name = LANG_NAMES.get(target_lang, target_lang)

    system_instruction = (
        f"You are an expert drama localization writer translating spoken {src_name} dialogue "
        f"into natural, conversational {tgt_name} for voice dubbing.\n"
        "Rules:\n"
        "1. Translate meaning, emotional tone, and interpersonal relationships naturally — NEVER translate literally word-for-word.\n"
        "2. Respect character gender presentation and hierarchy when selecting Khmer pronouns/honorifics.\n"
        "3. Keep line length speakable within each segment's duration window (start_ms to end_ms).\n"
        "4. Strictly follow any provided glossary terms and character names.\n"
        "5. Return one translation entry for EVERY input segment_id in the exact JSON schema."
    )

    payload_obj = {
        "source_language": src_name,
        "target_language": tgt_name,
        "scene_context": scene_context.strip(),
        "glossary": glossary,
        "preceding_dialogue_context": previous_context,
        "segments_to_translate": [
            {
                "segment_id": it.segment_id,
                "segment_index": it.segment_index,
                "speaker": it.speaker_name,
                "voice_presentation": it.voice_presentation,
                "duration_ms": max(100, it.end_ms - it.start_ms),
                "source_text": it.source_text,
            }
            for it in items
        ],
    }
    return system_instruction, json.dumps(payload_obj, ensure_ascii=False, indent=2)


class GeminiTranslationProvider(TranslationProvider):
    """Gemini translation adapter using Interactions API + Pydantic structured outputs."""

    provider_id = "gemini-translation"
    supported_pairs = SUPPORTED_PAIRS

    def translate_batch(
        self,
        items: list[TranslationItemInput],
        source_lang: str,
        target_lang: str,
        glossary: list[dict[str, str]],
        scene_context: str,
        previous_context: list[dict[str, str]],
        credentials: dict[str, str],
        options: dict[str, Any],
    ) -> list[TranslatedItemOutput]:
        if (source_lang, target_lang) not in self.supported_pairs:
            raise UnsupportedLanguageError(self.provider_id, source_lang, target_lang)

        api_key = str(credentials.get("GEMINI_API_KEY") or "").strip()
        if not api_key:
            raise MissingCredentialError(self.provider_id, "GEMINI_API_KEY")

        from google import genai

        model = str(options.get("model") or "gemini-4-hydrogen")
        system_instruction, user_prompt = build_translation_prompts(
            items=items,
            source_lang=source_lang,
            target_lang=target_lang,
            glossary=glossary,
            scene_context=scene_context,
            previous_context=previous_context,
        )

        client = genai.Client(api_key=api_key)
        try:
            interaction = client.interactions.create(
                model=model,
                input=user_prompt,
                system_instruction=system_instruction,
                response_format={
                    "type": "text",
                    "mime_type": "application/json",
                    "schema": _TranslationBatchSchema.model_json_schema(),
                },
                store=False,
            )
        except Exception as exc:
            msg = str(exc)
            lower = msg.lower()
            if "401" in lower or "403" in lower or "api key" in lower or "permission" in lower:
                raise MissingCredentialError(self.provider_id, "GEMINI_API_KEY") from exc
            retryable = any(k in lower for k in ("429", "503", "timeout", "rate", "quota"))
            raise TaskExecutionError(
                f"Gemini translation request failed: {msg}",
                retryable=retryable,
            ) from exc

        raw_text = getattr(interaction, "output_text", None) or ""
        if not raw_text.strip():
            raise TaskExecutionError("Gemini returned empty translation output", retryable=True)

        parsed = _TranslationBatchSchema.model_validate_json(raw_text)
        by_id = {t.segment_id: t for t in parsed.translations}
        outputs: list[TranslatedItemOutput] = []
        for it in items:
            matched = by_id.get(it.segment_id)
            if matched is None or not matched.translated_text.strip():
                raise TaskExecutionError(
                    f"Gemini omitted translation for segment '{it.segment_id}'",
                    retryable=True,
                )
            outputs.append(
                TranslatedItemOutput(
                    segment_id=it.segment_id,
                    translated_text=matched.translated_text.strip(),
                    notes=matched.notes.strip(),
                )
            )
        return outputs


def handle_translate(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    """Worker task handler for `translation.run`."""
    source_lang = str(payload.get("source_lang") or "zh").strip()
    target_lang = str(payload.get("target_lang") or "km").strip()
    output_path = str(payload.get("output_path") or "").strip()
    if not output_path:
        raise TaskExecutionError("'output_path' is required for translation.run")

    raw_segments = payload.get("segments") or []
    if not isinstance(raw_segments, list) or len(raw_segments) == 0:
        raise TaskExecutionError("No segments provided for translation")

    credentials = payload.get("credentials") or {}
    glossary = payload.get("glossary") or []
    scene_context = str(payload.get("scene_context") or "")
    batch_size = max(1, min(int(payload.get("batch_size") or 20), 50))
    model = str(payload.get("model") or "gemini-4-hydrogen")

    items = [
        TranslationItemInput(
            segment_id=str(s["id"]),
            segment_index=int(s.get("segment_index", i)),
            speaker_name=str(s.get("speaker_name") or "Unknown"),
            voice_presentation=str(s.get("voice_presentation") or "uncertain"),
            start_ms=int(s.get("start_ms", 0)),
            end_ms=int(s.get("end_ms", 1000)),
            source_text=str(s.get("source_text") or "").strip(),
        )
        for i, s in enumerate(raw_segments)
        if str(s.get("source_text") or "").strip()
    ]
    if not items:
        raise TaskExecutionError("All provided segments have empty source text")

    provider = GeminiTranslationProvider()
    if (source_lang, target_lang) not in provider.supported_pairs:
        raise UnsupportedLanguageError(provider.provider_id, source_lang, target_lang)
    if not str(credentials.get("GEMINI_API_KEY") or "").strip():
        raise MissingCredentialError(provider.provider_id, "GEMINI_API_KEY")

    all_results: list[dict[str, str]] = []
    total = len(items)
    for start in range(0, total, batch_size):
        ctx.check_cancelled()
        chunk = items[start : start + batch_size]
        prev_ctx = [
            {
                "speaker": r["speaker_name"],
                "source": r["source_text"],
                "translated": r["translated_text"],
            }
            for r in all_results[-3:]
        ]
        translated_chunk = provider.translate_batch(
            items=chunk,
            source_lang=source_lang,
            target_lang=target_lang,
            glossary=glossary,
            scene_context=scene_context,
            previous_context=prev_ctx,
            credentials=credentials,
            options={"model": model},
        )
        for inp, out in zip(chunk, translated_chunk, strict=True):
            all_results.append(
                {
                    "segment_id": out.segment_id,
                    "speaker_name": inp.speaker_name,
                    "source_text": inp.source_text,
                    "translated_text": out.translated_text,
                    "notes": out.notes,
                }
            )
        pct = min(99.0, ((start + len(chunk)) / total) * 100.0)
        ctx.emit_progress(pct, "translating", f"Translated {start + len(chunk)}/{total} lines")

    doc = {
        "provider_id": provider.provider_id,
        "model": model,
        "source_lang": source_lang,
        "target_lang": target_lang,
        "translations": [
            {
                "segment_id": r["segment_id"],
                "translated_text": r["translated_text"],
                "notes": r["notes"],
            }
            for r in all_results
        ],
    }
    out_file = Path(output_path)
    out_file.parent.mkdir(parents=True, exist_ok=True)
    out_file.write_text(json.dumps(doc, ensure_ascii=False, indent=2), encoding="utf-8")
    ctx.emit_progress(100.0, "completed", f"Translated {len(all_results)} segments")

    return {
        "output_path": str(out_file),
        "translated_count": len(all_results),
        "provider_id": provider.provider_id,
        "model": model,
    }
