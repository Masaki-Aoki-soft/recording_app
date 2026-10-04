/**
 * Tauri バックエンド通信
 *
 * 録画・Zoom 参加・アップロードのライフサイクルはすべて Rust 側で完結する。
 * フロントは状態の表示と設定の読み書きのみを担当する。
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// =====================================================
// 型定義
// =====================================================

export interface ScheduleType {
    type: 'Once' | 'Weekly';
    datetime?: string; // Once の場合 (ISO 8601)
    day_of_week?: number; // Weekly: 0=日, 1=月, ..., 6=土
    hour?: number; // Weekly
    minute?: number; // Weekly
}

export interface Schedule {
    id: string;
    name: string;
    url: string;
    schedule_type: ScheduleType;
    active: boolean;
    duration_minutes: number | null;
    /** 次回の開始予定日時 (ISO 8601)。一覧取得時に Rust 側で計算される */
    next_run?: string | null;
}

export type ScheduleInput = Omit<Schedule, 'id' | 'next_run'> & { id?: string };

export interface RecordingConfig {
    resolution: string; // "720p" | "1080p" | "4k"
    framerate: number;
    capture_system_audio: boolean;
    capture_mic: boolean;
    mic_device: string | null;
}

export interface GeneralSettings {
    /** Zoom 参加時の表示名 */
    zoom_display_name: string;
    /** 開始時刻の何秒前に Zoom を起動するか */
    lead_seconds: number;
    /** 会議ウィンドウが現れるまで待つ最大時間（分） */
    wait_timeout_minutes: number;
}

export interface DriveConfig {
    folder_name: string;
    delete_after_upload: boolean;
    auto_upload: boolean;
}

export interface DriveAuthStatus {
    connected: boolean;
    email: string | null;
}

export type SessionState =
    | 'idle'
    | 'launching'
    | 'waiting_for_meeting'
    | 'recording'
    | 'finalizing'
    | 'uploading'
    | 'error';

export interface SessionStatus {
    state: SessionState;
    schedule_name: string | null;
    /** 録画開始日時 (ISO 8601) */
    started_at: string | null;
    output_path: string | null;
    message: string | null;
}

export interface RecordingEntry {
    path: string;
    file_name: string;
    size_bytes: number;
    modified_at: string;
    uploaded: boolean;
}

export interface UploadProgressPayload {
    file_name: string;
    progress_percent: number;
    status: 'uploading' | 'completed' | 'error';
    message?: string | null;
}

// =====================================================
// スケジュール API
// =====================================================

export async function listSchedules(): Promise<Schedule[]> {
    return invoke<Schedule[]>('list_schedules');
}

export async function addSchedule(schedule: ScheduleInput): Promise<Schedule> {
    return invoke<Schedule>('add_schedule', {
        schedule: { ...schedule, id: schedule.id || '' },
    });
}

export async function updateSchedule(schedule: ScheduleInput & { id: string }): Promise<void> {
    return invoke('update_schedule', { schedule });
}

export async function deleteSchedule(id: string): Promise<void> {
    return invoke('delete_schedule', { id });
}

export async function toggleSchedule(id: string, active: boolean): Promise<void> {
    return invoke('toggle_schedule', { id, active });
}

/** スケジュールを今すぐ実行（Zoom 参加 → 録画） */
export async function runScheduleNow(id: string): Promise<void> {
    return invoke('run_schedule_now', { id });
}

// =====================================================
// 録画セッション API
// =====================================================

export async function getSessionStatus(): Promise<SessionStatus> {
    return invoke<SessionStatus>('get_session_status');
}

/** 手動録画を開始（Zoom 会議ウィンドウがあればそれを、なければ画面全体を録画） */
export async function startManualRecording(): Promise<void> {
    return invoke('start_manual_recording');
}

/** 実行中のセッション（録画 / 会議待ち）を停止 */
export async function stopSession(): Promise<void> {
    return invoke('stop_session');
}

export async function listRecordings(): Promise<RecordingEntry[]> {
    return invoke<RecordingEntry[]>('list_recordings');
}

export async function openRecordingsDir(): Promise<void> {
    return invoke('open_recordings_dir');
}

// =====================================================
// 設定 API
// =====================================================

export async function getRecordingConfig(): Promise<RecordingConfig> {
    return invoke<RecordingConfig>('get_recording_config');
}

export async function saveRecordingConfig(config: RecordingConfig): Promise<void> {
    return invoke('save_recording_config', { config });
}

export async function getGeneralSettings(): Promise<GeneralSettings> {
    return invoke<GeneralSettings>('get_general_settings');
}

export async function saveGeneralSettings(settings: GeneralSettings): Promise<void> {
    return invoke('save_general_settings', { settings });
}

export async function getMicDevices(): Promise<string[]> {
    return invoke<string[]>('get_mic_devices');
}

/** Clerk のサインイン状態を Rust 側へ通知（サインイン中のみ自動録画する） */
export async function setSignedIn(signedIn: boolean): Promise<void> {
    return invoke('set_signed_in', { signedIn });
}

// =====================================================
// ソーシャルログイン（Clerk SSO）
// =====================================================

/** Clerk の signIn.create に渡す redirectUrl（Rust のループバックサーバー） */
export async function getSsoRedirectUrl(): Promise<string> {
    return invoke<string>('sso_redirect_url');
}

/** 認証 URL をシステムブラウザで開き、Clerk からのコールバック URL を待つ */
export async function waitForSsoCallback(authUrl: string): Promise<string> {
    return invoke<string>('wait_for_sso_callback', { authUrl });
}

export async function cancelSso(): Promise<void> {
    return invoke('cancel_sso');
}

// =====================================================
// Google Drive API
// =====================================================

export async function connectGoogleDrive(): Promise<void> {
    return invoke('start_google_auth');
}

export async function disconnectGoogleDrive(): Promise<void> {
    return invoke('disconnect_google');
}

export async function getDriveAuthStatus(): Promise<DriveAuthStatus> {
    return invoke<DriveAuthStatus>('get_drive_auth_status');
}

export async function getDriveConfig(): Promise<DriveConfig> {
    return invoke<DriveConfig>('get_drive_config');
}

export async function setDriveConfig(config: DriveConfig): Promise<void> {
    return invoke('set_drive_config', { config });
}

/** 録画ファイルを Drive へアップロード（キューに追加） */
export async function uploadRecording(path: string): Promise<void> {
    return invoke('upload_recording', { path });
}

// =====================================================
// イベントリスナー
// =====================================================

export function onSessionStatus(callback: (status: SessionStatus) => void): Promise<UnlistenFn> {
    return listen<SessionStatus>('session-status', (event) => callback(event.payload));
}

export function onUploadProgress(
    callback: (payload: UploadProgressPayload) => void,
): Promise<UnlistenFn> {
    return listen<UploadProgressPayload>('upload-progress', (event) => callback(event.payload));
}

export function onSchedulesChanged(callback: () => void): Promise<UnlistenFn> {
    return listen('schedules-changed', () => callback());
}

// =====================================================
// ユーティリティ
// =====================================================

export function isTauri(): boolean {
    return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** 秒数を MM:SS / HH:MM:SS 形式にフォーマット */
export function formatElapsedTime(seconds: number): string {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;

    if (h > 0) {
        return `${h.toString().padStart(2, '0')}:${m
            .toString()
            .padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
    }
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
}

export function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    const units = ['KB', 'MB', 'GB', 'TB'];
    let value = bytes / 1024;
    let i = 0;
    while (value >= 1024 && i < units.length - 1) {
        value /= 1024;
        i++;
    }
    return `${value.toFixed(1)} ${units[i]}`;
}

/** Tauri の invoke エラー（文字列）を表示用メッセージに変換 */
export function errorMessage(err: unknown): string {
    if (typeof err === 'string') return err;
    if (err instanceof Error) return err.message;
    return String(err);
}
