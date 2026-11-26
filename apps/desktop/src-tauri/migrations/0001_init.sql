-- XakuStudio initial SQLite schema (v1.0 foundation)

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    root_path TEXT NOT NULL UNIQUE,
    source_lang TEXT NOT NULL DEFAULT 'zh',
    target_lang TEXT NOT NULL DEFAULT 'km',
    preset TEXT NOT NULL DEFAULT 'balanced',
    settings_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS media_assets (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    file_path TEXT NOT NULL,
    sha256 TEXT,
    duration_ms INTEGER,
    width INTEGER,
    height INTEGER,
    fps_num INTEGER,
    fps_den INTEGER,
    audio_channels INTEGER,
    sample_rate INTEGER,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_media_assets_project_role ON media_assets(project_id, role);

CREATE TABLE IF NOT EXISTS speakers (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    diarization_label TEXT NOT NULL,
    display_name TEXT NOT NULL,
    voice_gender_est TEXT NOT NULL DEFAULT 'uncertain',
    gender_confidence REAL NOT NULL DEFAULT 0.0,
    pitch_hz_avg REAL,
    voice_provider_id TEXT,
    voice_id TEXT,
    voice_settings_json TEXT NOT NULL DEFAULT '{}',
    is_user_overridden INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_speakers_project ON speakers(project_id);

CREATE TABLE IF NOT EXISTS dialogue_segments (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    segment_index INTEGER NOT NULL,
    speaker_id TEXT REFERENCES speakers(id) ON DELETE SET NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    source_text TEXT NOT NULL DEFAULT '',
    source_text_original TEXT NOT NULL DEFAULT '',
    words_json TEXT NOT NULL DEFAULT '[]',
    emotion_est TEXT NOT NULL DEFAULT 'neutral',
    emotion_confidence REAL NOT NULL DEFAULT 0.0,
    translated_text TEXT NOT NULL DEFAULT '',
    translated_text_auto TEXT NOT NULL DEFAULT '',
    translation_status TEXT NOT NULL DEFAULT 'pending',
    tts_provider_id TEXT,
    tts_voice_id TEXT,
    tts_audio_path TEXT,
    tts_duration_ms INTEGER,
    sync_mode TEXT NOT NULL DEFAULT 'fit_tempo',
    speed_ratio REAL NOT NULL DEFAULT 1.0,
    gain_db REAL NOT NULL DEFAULT 0.0,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_dialogue_segments_project_idx ON dialogue_segments(project_id, segment_index);

CREATE TABLE IF NOT EXISTS subtitle_styles (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    font_family TEXT NOT NULL DEFAULT 'Noto Sans Khmer',
    font_size INTEGER NOT NULL DEFAULT 24,
    font_weight INTEGER NOT NULL DEFAULT 600,
    text_color TEXT NOT NULL DEFAULT '#FFFFFF',
    outline_color TEXT NOT NULL DEFAULT '#000000',
    outline_width REAL NOT NULL DEFAULT 2.0,
    shadow_color TEXT NOT NULL DEFAULT '#000000',
    shadow_depth REAL NOT NULL DEFAULT 1.0,
    bg_opacity REAL NOT NULL DEFAULT 0.45,
    alignment INTEGER NOT NULL DEFAULT 2,
    margin_v INTEGER NOT NULL DEFAULT 36,
    is_default INTEGER NOT NULL DEFAULT 0
);

INSERT OR IGNORE INTO subtitle_styles (
    id, project_id, name, font_family, font_size, font_weight,
    text_color, outline_color, outline_width, shadow_color, shadow_depth,
    bg_opacity, alignment, margin_v, is_default
) VALUES (
    'default-khmer', NULL, 'Khmer Drama Default', 'Noto Sans Khmer', 24, 600,
    '#FFFFFF', '#000000', 2.0, '#000000', 1.0,
    0.45, 2, 36, 1
);

CREATE TABLE IF NOT EXISTS subtitles (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    segment_id TEXT REFERENCES dialogue_segments(id) ON DELETE SET NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    text TEXT NOT NULL,
    style_id TEXT REFERENCES subtitle_styles(id) ON DELETE SET NULL,
    position_override_json TEXT
);
CREATE INDEX IF NOT EXISTS idx_subtitles_project_time ON subtitles(project_id, start_ms);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    pipeline_run_id TEXT,
    task_type TEXT NOT NULL,
    resource_class TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    progress REAL NOT NULL DEFAULT 0.0,
    stage_message TEXT NOT NULL DEFAULT '',
    input_json TEXT NOT NULL DEFAULT '{}',
    output_json TEXT NOT NULL DEFAULT '{}',
    error_code TEXT,
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 2,
    started_at TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status, created_at);

CREATE TABLE IF NOT EXISTS task_dependencies (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    depends_on_task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, depends_on_task_id)
);

CREATE TABLE IF NOT EXISTS glossary_entries (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    source_term TEXT NOT NULL,
    target_term TEXT NOT NULL,
    notes TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS downloads (
    id TEXT PRIMARY KEY NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    url TEXT NOT NULL,
    dest_path TEXT NOT NULL,
    total_bytes INTEGER,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    sha256_expected TEXT,
    status TEXT NOT NULL DEFAULT 'queued',
    error_message TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS exports (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    output_path TEXT NOT NULL,
    format TEXT NOT NULL DEFAULT 'mp4',
    config_json TEXT NOT NULL DEFAULT '{}',
    qa_report_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending',
    file_size_bytes INTEGER,
    duration_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS provider_configs (
    provider_id TEXT PRIMARY KEY NOT NULL,
    capability TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    config_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
