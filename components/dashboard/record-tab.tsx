'use client';

import { useCallback, useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import {
    AlertCircle,
    CheckCircle,
    CloudUpload,
    FolderOpen,
    HardDrive,
    Loader2,
    Mic,
    Monitor,
    Play,
    RefreshCw,
    Square,
} from 'lucide-react';
import toast from 'react-hot-toast';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import {
    errorMessage,
    formatBytes,
    formatElapsedTime,
    getDriveAuthStatus,
    getMicDevices,
    getRecordingConfig,
    isTauri,
    listRecordings,
    openRecordingsDir,
    saveRecordingConfig,
    startManualRecording,
    stopSession,
    uploadRecording,
    type RecordingConfig,
    type RecordingEntry,
    type SessionState,
    type SessionStatus,
} from '@/lib/tauri';

const DEFAULT_MIC = '__default__';

const STATE_LABELS: Record<SessionState, string> = {
    idle: '待機中',
    launching: 'Zoom を起動しています...',
    waiting_for_meeting: '会議の開始を待っています...',
    recording: '録画中',
    finalizing: '動画ファイルを保存しています...',
    uploading: 'Google Drive にアップロードしています...',
    error: 'エラー',
};

export default function RecordTab({ session, elapsed }: { session: SessionStatus; elapsed: number }) {
    const [config, setConfig] = useState<RecordingConfig | null>(null);
    const [mics, setMics] = useState<string[]>([]);
    const [recordings, setRecordings] = useState<RecordingEntry[]>([]);
    const [driveConnected, setDriveConnected] = useState(false);
    const [uploading, setUploading] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);

    const refreshRecordings = useCallback(async () => {
        if (!isTauri()) return;
        try {
            setRecordings(await listRecordings());
        } catch (err) {
            console.error('Failed to list recordings:', err);
        }
    }, []);

    useEffect(() => {
        if (!isTauri()) return;
        getRecordingConfig().then(setConfig).catch(console.error);
        getMicDevices().then(setMics).catch(console.error);
        getDriveAuthStatus()
            .then((s) => setDriveConnected(s.connected))
            .catch(console.error);
    }, []);

    // セッション終了時（idle/error に戻ったとき）に一覧を更新
    useEffect(() => {
        if (session.state === 'idle' || session.state === 'error') {
            refreshRecordings();
        }
    }, [session.state, refreshRecordings]);

    const updateConfig = async (patch: Partial<RecordingConfig>) => {
        if (!config) return;
        const next = { ...config, ...patch };
        setConfig(next);
        try {
            await saveRecordingConfig(next);
        } catch (err) {
            toast.error(`設定を保存できませんでした: ${errorMessage(err)}`);
        }
    };

    const isActive = ['launching', 'waiting_for_meeting', 'recording'].includes(session.state);
    const isFinishing = session.state === 'finalizing' || session.state === 'uploading';

    const handleToggle = async () => {
        setBusy(true);
        try {
            if (isActive) {
                await stopSession();
                toast('停止しています...', { icon: '⏹️' });
            } else {
                await startManualRecording();
                toast.success('録画を開始します');
            }
        } catch (err) {
            toast.error(errorMessage(err));
        } finally {
            setBusy(false);
        }
    };

    const handleUpload = async (entry: RecordingEntry) => {
        setUploading(entry.path);
        try {
            await uploadRecording(entry.path);
            await refreshRecordings();
        } catch (err) {
            toast.error(`アップロードに失敗しました: ${errorMessage(err)}`);
        } finally {
            setUploading(null);
        }
    };

    return (
        <motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} className="space-y-6">
            <div>
                <h2 className="text-2xl md:text-3xl font-bold tracking-tight">録画</h2>
                <p className="text-sm md:text-base text-zinc-500 mt-2">
                    Zoom の会議ウィンドウ（見つからない場合は画面全体）と音声を録画します。
                </p>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                {/* 録画コントローラー */}
                <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                    <CardHeader>
                        <CardTitle className="text-lg">録画コントロール</CardTitle>
                        {session.schedule_name && (
                            <CardDescription>{session.schedule_name}</CardDescription>
                        )}
                    </CardHeader>
                    <CardContent className="flex flex-col items-center justify-center py-6">
                        <div className="relative">
                            {session.state === 'recording' && (
                                <span className="absolute -top-2 -right-2 flex h-4 w-4">
                                    <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-red-400 opacity-75"></span>
                                    <span className="relative inline-flex rounded-full h-4 w-4 bg-red-500"></span>
                                </span>
                            )}
                            <Button
                                size="lg"
                                className={`h-24 w-24 rounded-full shadow-lg cursor-pointer transition-all duration-300 ${
                                    isActive
                                        ? 'bg-zinc-900 hover:bg-zinc-800 dark:bg-white dark:hover:bg-zinc-200 text-red-500'
                                        : 'bg-blue-600 hover:bg-blue-700 text-white'
                                }`}
                                onClick={handleToggle}
                                disabled={!isTauri() || busy || isFinishing}
                            >
                                {isFinishing || busy ? (
                                    <Loader2 className="h-10 w-10 animate-spin" />
                                ) : isActive ? (
                                    <Square className="h-10 w-10 fill-current" />
                                ) : (
                                    <Play className="h-10 w-10 fill-current ml-2" />
                                )}
                            </Button>
                        </div>
                        <p className="mt-6 text-sm font-medium text-zinc-600 dark:text-zinc-400">
                            {session.state === 'recording'
                                ? `録画中... (${formatElapsedTime(elapsed)})`
                                : session.state === 'idle'
                                  ? 'ボタンを押して録画開始'
                                  : STATE_LABELS[session.state]}
                        </p>
                        {session.message && (
                            <p
                                className={`mt-3 text-xs text-center flex items-start gap-1 ${
                                    session.state === 'error' ? 'text-red-600' : 'text-zinc-500'
                                }`}
                            >
                                {session.state === 'error' ? (
                                    <AlertCircle className="h-3.5 w-3.5 shrink-0 mt-px" />
                                ) : (
                                    <CheckCircle className="h-3.5 w-3.5 shrink-0 mt-px" />
                                )}
                                {session.message}
                            </p>
                        )}
                    </CardContent>
                </Card>

                {/* 入力ソース設定 */}
                <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                    <CardHeader>
                        <CardTitle className="text-lg">入力ソース</CardTitle>
                        <CardDescription>次の録画から反映されます</CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-6">
                        <div className="flex items-center justify-between">
                            <div className="flex items-center space-x-3">
                                <Monitor className="h-5 w-5 text-zinc-500" />
                                <Label className="text-sm md:text-base">映像</Label>
                            </div>
                            <Badge variant="outline" className="bg-blue-50 text-blue-700 border-blue-200">
                                {config ? `${config.resolution} / ${config.framerate}fps` : '-'}
                            </Badge>
                        </div>
                        <div className="flex items-center justify-between">
                            <div className="flex items-center space-x-3">
                                <HardDrive className="h-5 w-5 text-zinc-500" />
                                <div className="space-y-0.5">
                                    <Label htmlFor="system-audio" className="text-sm md:text-base cursor-pointer">
                                        システム音声（相手の声）
                                    </Label>
                                    <p className="text-xs text-zinc-500">WASAPI ループバック</p>
                                </div>
                            </div>
                            <Switch
                                id="system-audio"
                                checked={config?.capture_system_audio ?? false}
                                disabled={!config}
                                onCheckedChange={(checked) => updateConfig({ capture_system_audio: checked })}
                            />
                        </div>
                        <div className="space-y-3">
                            <div className="flex items-center justify-between">
                                <div className="flex items-center space-x-3">
                                    <Mic className="h-5 w-5 text-zinc-500" />
                                    <Label htmlFor="mic" className="text-sm md:text-base cursor-pointer">
                                        マイク（自分の声）
                                    </Label>
                                </div>
                                <Switch
                                    id="mic"
                                    checked={config?.capture_mic ?? false}
                                    disabled={!config}
                                    onCheckedChange={(checked) => updateConfig({ capture_mic: checked })}
                                />
                            </div>
                            {config?.capture_mic && (
                                <Select
                                    value={config.mic_device ?? DEFAULT_MIC}
                                    onValueChange={(v) =>
                                        updateConfig({ mic_device: v === DEFAULT_MIC ? null : v })
                                    }
                                >
                                    <SelectTrigger className="w-full">
                                        <SelectValue placeholder="マイクを選択" />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value={DEFAULT_MIC}>既定のマイク</SelectItem>
                                        {mics.map((name) => (
                                            <SelectItem key={name} value={name}>
                                                {name}
                                            </SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            )}
                        </div>
                    </CardContent>
                </Card>
            </div>

            {/* 録画一覧 */}
            <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                <CardHeader>
                    <div className="flex items-center justify-between gap-2">
                        <CardTitle className="text-lg">録画ファイル</CardTitle>
                        <div className="flex gap-2">
                            <Button variant="outline" size="sm" className="cursor-pointer" onClick={refreshRecordings}>
                                <RefreshCw className="h-4 w-4" />
                            </Button>
                            <Button
                                variant="outline"
                                size="sm"
                                className="cursor-pointer"
                                disabled={!isTauri()}
                                onClick={() => openRecordingsDir().catch((e) => toast.error(errorMessage(e)))}
                            >
                                <FolderOpen className="mr-2 h-4 w-4" /> フォルダを開く
                            </Button>
                        </div>
                    </div>
                </CardHeader>
                <CardContent className="p-0">
                    {recordings.length === 0 ? (
                        <p className="p-6 text-center text-sm text-zinc-400">録画ファイルはまだありません</p>
                    ) : (
                        <div className="divide-y divide-zinc-200 dark:divide-zinc-800">
                            {recordings.map((entry) => (
                                <div key={entry.path} className="px-6 py-3 flex items-center justify-between gap-4">
                                    <div className="min-w-0">
                                        <p className="text-sm font-medium truncate" title={entry.file_name}>
                                            {entry.file_name}
                                        </p>
                                        <p className="text-xs text-zinc-500">
                                            {new Date(entry.modified_at).toLocaleString('ja-JP')} ・{' '}
                                            {formatBytes(entry.size_bytes)}
                                        </p>
                                    </div>
                                    {entry.uploaded ? (
                                        <Badge className="bg-green-100 text-green-700 border-green-200 shrink-0">
                                            <CheckCircle className="h-3 w-3 mr-1" /> Drive 保存済み
                                        </Badge>
                                    ) : (
                                        driveConnected && (
                                            <Button
                                                variant="outline"
                                                size="sm"
                                                className="cursor-pointer shrink-0"
                                                disabled={uploading !== null}
                                                onClick={() => handleUpload(entry)}
                                            >
                                                {uploading === entry.path ? (
                                                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                                                ) : (
                                                    <CloudUpload className="mr-2 h-4 w-4" />
                                                )}
                                                アップロード
                                            </Button>
                                        )
                                    )}
                                </div>
                            ))}
                        </div>
                    )}
                </CardContent>
            </Card>
        </motion.div>
    );
}
