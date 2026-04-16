/**
 * Tauri バックエンド通信 + FFmpeg 録画制御
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// =====================================================
// 型定義
// =====================================================

export interface ScheduleType {
    type: 'Once' | 'Weekly';
    datetime?: string; // Once の場合
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
}

export interface RecordingConfig {
    resolution: string;
    framerate: number;
    capture_system_audio: boolean;
    capture_mic: boolean;
    audio_device: string | null;
    mic_device: string | null;
}

export interface DriveConfig {
    folder_name: string;
    delete_after_upload: boolean;
}

export interface AuthStatus {
    is_authenticated: boolean;
    user_email: string | null;
}

export interface ScheduleTriggeredPayload {
    schedule_id: string;
    schedule_name: string;
    url: string;
    duration_minutes: number | null;
}

export interface UploadProgressPayload {
    file_name: string;
    progress_percent: number;
    status: 'uploading' | 'completed' | 'error';
}

// =====================================================
// スケジュール API
// =====================================================

export async function listSchedules(): Promise<Schedule[]> {
    return invoke<Schedule[]>('list_schedules');
}

export async function addSchedule(schedule: Omit<Schedule, 'id'> & { id?: string }): Promise<Schedule> {
    return invoke<Schedule>('add_schedule', {
        schedule: { ...schedule, id: schedule.id || '' },
    });
}

export async function updateSchedule(schedule: Schedule): Promise<void> {
    return invoke('update_schedule', { schedule });
}

export async function deleteSchedule(id: string): Promise<void> {
    return invoke('delete_schedule', { id });
}

export async function toggleSchedule(id: string, active: boolean): Promise<void> {
    return invoke('toggle_schedule', { id, active });
}

// =====================================================
// Google Drive API
// =====================================================

export async function startGoogleAuth(): Promise<string> {
    return invoke<string>('start_google_auth');
}

export async function getAuthStatus(): Promise<AuthStatus> {
    return invoke<AuthStatus>('get_auth_status');
}

export async function uploadToDrive(filePath: string, fileName: string): Promise<void> {
    return invoke('upload_to_drive', { filePath, fileName });
}

export async function getDriveConfig(): Promise<DriveConfig> {
    return invoke<DriveConfig>('get_drive_config');
}

export async function setDriveConfig(config: DriveConfig): Promise<void> {
    return invoke('set_drive_config', { config });
}

// =====================================================
// 録画設定 API
// =====================================================

export async function getRecordingConfig(): Promise<RecordingConfig> {
    return invoke<RecordingConfig>('get_recording_config');
}

export async function saveRecordingConfig(config: RecordingConfig): Promise<void> {
    return invoke('save_recording_config', { config });
}

export async function getRecordingsDir(): Promise<string> {
    return invoke<string>('get_recordings_dir');
}

export async function getAudioDevices(): Promise<string[]> {
    return invoke<string[]>('get_audio_devices');
}

// =====================================================
// FFmpeg 録画マネージャー（@tauri-apps/plugin-shell で制御）
// =====================================================

export type RecordingStatus = 'idle' | 'recording' | 'stopping' | 'uploading';

export class RecordingManager {
    private startTime: Date | null = null;
    private outputPath: string = '';
    private _status: RecordingStatus = 'idle';
    private onStatusChange?: (status: RecordingStatus) => void;
    private onError?: (error: string) => void;
    private onLog?: (line: string) => void;

    constructor(options?: {
        onStatusChange?: (status: RecordingStatus) => void;
        onError?: (error: string) => void;
        onLog?: (line: string) => void;
    }) {
        this.onStatusChange = options?.onStatusChange;
        this.onError = options?.onError;
        this.onLog = options?.onLog;
    }

    get status(): RecordingStatus {
        return this._status;
    }

    get recordingStartTime(): Date | null {
        return this.startTime;
    }

    get recordingFilePath(): string {
        return this.outputPath;
    }

    private setStatus(status: RecordingStatus) {
        this._status = status;
        this.onStatusChange?.(status);
    }

    /**
     * 録画を開始
     */
    async start(config: RecordingConfig): Promise<string> {
        if (this._status === 'recording') {
            throw new Error('Already recording');
        }

        try {
            console.log('Starting FFmpeg via Rust backend...');
            const outputPath = await invoke<string>('start_recording', { config });
            this.outputPath = outputPath;
            this.startTime = new Date();
            this.setStatus('recording');
            
            console.log('FFmpeg recording started:', this.outputPath);
            return this.outputPath;
        } catch (error) {
            const msg = error instanceof Error ? error.message : String(error);
            this.onError?.(msg);
            throw error;
        }
    }

    /**
     * 録画を正常停止（FFmpeg に 'q' を送信）
     */
    async stop(): Promise<string> {
        if (this._status !== 'recording') {
            throw new Error('Not recording');
        }

        this.setStatus('stopping');

        try {
            const outputPath = await invoke<string>('stop_recording');
            this.outputPath = outputPath;
        } catch (error) {
            console.error('Failed to stop recording:', error);
            // ignore
        }

        // プロセス終了を少し待つ (UIのチラツキ防止)
        await new Promise((resolve) => setTimeout(resolve, 500));

        this.setStatus('idle');

        return this.outputPath;
    }

    /**
     * 録画を強制終了
     */
    async kill(): Promise<void> {
        if (this._status === 'recording') {
            try {
                await invoke<string>('stop_recording');
            } catch {
                // ignore
            }
            this.setStatus('idle');
        }
    }

    /**
     * 経過時間を取得（秒）
     */
    getElapsedSeconds(): number {
        if (!this.startTime) return 0;
        return Math.floor((Date.now() - this.startTime.getTime()) / 1000);
    }
}

// =====================================================
// イベントリスナー
// =====================================================

/**
 * スケジュール発火イベントをリッスン
 */
export function onScheduleTriggered(
    callback: (payload: ScheduleTriggeredPayload) => void,
): Promise<UnlistenFn> {
    return listen<ScheduleTriggeredPayload>('schedule-triggered', (event) => {
        callback(event.payload);
    });
}

/**
 * アップロード進捗イベントをリッスン
 */
export function onUploadProgress(
    callback: (payload: UploadProgressPayload) => void,
): Promise<UnlistenFn> {
    return listen<UploadProgressPayload>('upload-progress', (event) => {
        callback(event.payload);
    });
}

// =====================================================
// ユーティリティ
// =====================================================

/**
 * 秒数を MM:SS 形式にフォーマット
 */
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
