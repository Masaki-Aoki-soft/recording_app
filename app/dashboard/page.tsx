'use client';

import { useState, useEffect, useRef, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import { motion, AnimatePresence } from 'framer-motion';
import {
    Video,
    Calendar,
    Settings,
    LogOut,
    Play,
    Square,
    Monitor,
    Mic,
    HardDrive,
    Plus,
    Clock,
    ExternalLink,
    CloudUpload,
    User,
    Trash2,
    Edit,
    Loader2,
    CheckCircle,
    AlertCircle,
} from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from '@/components/ui/select';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import toast from 'react-hot-toast';

import ScheduleDialog from '@/components/schedule-dialog';
import {
    RecordingManager,
    formatElapsedTime,
    listSchedules,
    addSchedule,
    deleteSchedule,
    toggleSchedule,
    startGoogleAuth,
    getAuthStatus,
    uploadToDrive,
    getDriveConfig,
    setDriveConfig,
    getRecordingConfig,
    saveRecordingConfig,
    onScheduleTriggered,
    onUploadProgress,
    type Schedule,
    type RecordingConfig,
    type DriveConfig,
    type AuthStatus,
    type RecordingStatus,
} from '@/lib/tauri';

export default function DashboardPage() {
    const router = useRouter();
    const [activeTab, setActiveTab] = useState<'record' | 'schedule' | 'settings'>('record');

    // サイドバーの開閉状態
    const [isSidebarOpen, setIsSidebarOpen] = useState(false);
    const menuRef = useRef<HTMLDivElement>(null);
    const buttonRef = useRef<HTMLDivElement>(null);

    // --- 録画関連 ---
    const [recordingStatus, setRecordingStatus] = useState<RecordingStatus>('idle');
    const [elapsedSeconds, setElapsedSeconds] = useState(0);
    const [recordingManager] = useState(() =>
        new RecordingManager({
            onStatusChange: (status) => setRecordingStatus(status),
            onError: (error) => toast.error(`録画エラー: ${error}`),
            onLog: (line) => console.log(line),
        }),
    );

    // --- 録画設定 ---
    const [recordingConfig, setRecordingConfig] = useState<RecordingConfig>({
        resolution: '1080p',
        framerate: 30,
        capture_system_audio: true,
        capture_mic: true,
        audio_device: null,
    });

    // --- スケジュール関連 ---
    const [schedules, setSchedules] = useState<Schedule[]>([]);
    const [showScheduleDialog, setShowScheduleDialog] = useState(false);
    const [editingSchedule, setEditingSchedule] = useState<Schedule | null>(null);

    // --- Google Drive 関連 ---
    const [authStatus, setAuthStatus] = useState<AuthStatus>({
        is_authenticated: false,
        user_email: null,
    });
    const [driveConfig, setDriveConfigState] = useState<DriveConfig>({
        folder_name: 'Meeting Records',
        delete_after_upload: false,
    });
    const [isAuthenticating, setIsAuthenticating] = useState(false);

    // Tauri APIが使えるかチェック（ブラウザプレビュー対応）
    const [isTauri, setIsTauri] = useState(false);

    useEffect(() => {
        // @ts-expect-error - __TAURI__ is injected by Tauri runtime
        setIsTauri(typeof window !== 'undefined' && !!window.__TAURI__);
    }, []);

    const [audioDevices, setAudioDevices] = useState<string[]>([]);

    // --- 初期データ読み込み ---
    useEffect(() => {
        if (!isTauri) return;

        const loadData = async () => {
            try {
                const [loadedSchedules, config, driveConf, auth, devices] = await Promise.all([
                    listSchedules(),
                    getRecordingConfig(),
                    getDriveConfig(),
                    getAuthStatus(),
                    import('@/lib/tauri').then(m => m.getAudioDevices()),
                ]);
                setSchedules(loadedSchedules);
                setRecordingConfig(config);
                setDriveConfigState(driveConf);
                setAuthStatus(auth);
                setAudioDevices(devices);
            } catch (err) {
                console.error('Failed to load initial data:', err);
            }
        };

        loadData();
    }, [isTauri]);

    // --- イベントリスナー ---
    useEffect(() => {
        if (!isTauri) return;

        let unlistenSchedule: (() => void) | undefined;
        let unlistenUpload: (() => void) | undefined;

        const setup = async () => {
            // スケジュール発火イベント: 自動録画開始
            unlistenSchedule = await onScheduleTriggered(async (payload) => {
                toast.success(`スケジュール「${payload.schedule_name}」を開始しました`, {
                    icon: '📹',
                });

                try {
                    const filePath = await recordingManager.start(recordingConfig);
                    toast.success('自動録画を開始しました');

                    // 時間指定がある場合、自動停止タイマー
                    if (payload.duration_minutes) {
                        setTimeout(async () => {
                            try {
                                const path = await recordingManager.stop();
                                toast.success('録画を自動停止しました');

                                // Google Driveにアップロード
                                if (authStatus.is_authenticated) {
                                    const fileName = path.split('\\').pop() || 'recording.mp4';
                                    toast('Google Drive にアップロード中...', { icon: '☁️' });
                                    await uploadToDrive(path, fileName);
                                }
                            } catch (err) {
                                console.error('Auto-stop failed:', err);
                            }
                        }, payload.duration_minutes * 60 * 1000);
                    }
                } catch (err) {
                    toast.error('自動録画の開始に失敗しました');
                    console.error(err);
                }
            });

            // アップロード進捗イベント
            unlistenUpload = await onUploadProgress((payload) => {
                if (payload.status === 'completed') {
                    toast.success(`${payload.file_name} をアップロード完了`);
                } else if (payload.status === 'error') {
                    toast.error(`${payload.file_name} のアップロードに失敗`);
                }
            });
        };

        setup();

        return () => {
            unlistenSchedule?.();
            unlistenUpload?.();
        };
    }, [isTauri, recordingConfig, authStatus.is_authenticated]);

    // --- 録画経過時間の更新 ---
    useEffect(() => {
        if (recordingStatus !== 'recording') {
            setElapsedSeconds(0);
            return;
        }

        const interval = setInterval(() => {
            setElapsedSeconds(recordingManager.getElapsedSeconds());
        }, 1000);

        return () => clearInterval(interval);
    }, [recordingStatus, recordingManager]);

    // --- ハンドラー ---
    const handleLogout = async () => {
        toast.success('ログアウトしました');
        router.push('/login');
    };

    const handleToggleRecording = async () => {
        if (recordingStatus === 'recording') {
            // 録画停止
            try {
                const filePath = await recordingManager.stop();
                toast.success('録画を停止しました');

                // Google Drive にアップロード
                if (isTauri && authStatus.is_authenticated) {
                    const fileName = filePath.split('\\').pop() || 'recording.mp4';
                    toast('Google Drive へアップロード中...', { icon: '☁️' });
                    await uploadToDrive(filePath, fileName);
                }
            } catch (err) {
                toast.error('録画停止に失敗しました');
                console.error(err);
            }
        } else {
            // 録画開始
            try {
                await recordingManager.start(recordingConfig);
                toast.success('録画を開始しました');
            } catch (err) {
                toast.error('録画開始に失敗しました。FFmpegがPATHにあるか確認してください。');
                console.error(err);
            }
        }
    };

    const handleAddSchedule = async (schedule: Omit<Schedule, 'id'> & { id?: string }) => {
        if (!isTauri) return;
        try {
            const created = await addSchedule(schedule);
            setSchedules((prev) => [...prev, created]);
            toast.success('スケジュールを追加しました');
        } catch (err) {
            toast.error('スケジュールの追加に失敗しました');
            console.error(err);
        }
    };

    const handleDeleteSchedule = async (id: string) => {
        if (!isTauri) return;
        try {
            await deleteSchedule(id);
            setSchedules((prev) => prev.filter((s) => s.id !== id));
            toast.success('スケジュールを削除しました');
        } catch (err) {
            toast.error('削除に失敗しました');
        }
    };

    const handleToggleSchedule = async (id: string, active: boolean) => {
        if (!isTauri) return;
        try {
            await toggleSchedule(id, active);
            setSchedules((prev) =>
                prev.map((s) => (s.id === id ? { ...s, active } : s)),
            );
        } catch (err) {
            toast.error('切り替えに失敗しました');
        }
    };

    const handleGoogleAuth = async () => {
        if (!isTauri) return;
        setIsAuthenticating(true);
        try {
            await startGoogleAuth();
            const status = await getAuthStatus();
            setAuthStatus(status);
            toast.success('Google Drive と連携しました');
        } catch (err) {
            toast.error('認証に失敗しました');
            console.error(err);
        } finally {
            setIsAuthenticating(false);
        }
    };

    const handleSaveDriveConfig = async (config: DriveConfig) => {
        if (!isTauri) return;
        try {
            await setDriveConfig(config);
            setDriveConfigState(config);
            toast.success('設定を保存しました');
        } catch (err) {
            toast.error('設定の保存に失敗しました');
        }
    };

    const handleSaveRecordingConfig = async (config: RecordingConfig) => {
        setRecordingConfig(config);
        if (!isTauri) return;
        try {
            await saveRecordingConfig(config);
        } catch (err) {
            console.error('Failed to save recording config:', err);
        }
    };

    // メニュー外をクリックしたときに閉じる処理
    useEffect(() => {
        const handleClickOutside = (event: MouseEvent) => {
            if (
                isSidebarOpen &&
                menuRef.current &&
                !menuRef.current.contains(event.target as Node) &&
                buttonRef.current &&
                !buttonRef.current.contains(event.target as Node)
            ) {
                setIsSidebarOpen(false);
            }
        };

        document.addEventListener('mousedown', handleClickOutside);
        return () => {
            document.removeEventListener('mousedown', handleClickOutside);
        };
    }, [isSidebarOpen]);

    // スケジュール表示用ヘルパー
    const formatScheduleTime = (schedule: Schedule): string => {
        const st = schedule.schedule_type;
        if (st.type === 'Once' && st.datetime) {
            const d = new Date(st.datetime);
            return d.toLocaleDateString('ja-JP') + ' ' + d.toLocaleTimeString('ja-JP', { hour: '2-digit', minute: '2-digit' });
        } else if (st.type === 'Weekly') {
            const days = ['日', '月', '火', '水', '木', '金', '土'];
            return `毎週${days[st.day_of_week ?? 0]}曜日 ${String(st.hour ?? 0).padStart(2, '0')}:${String(st.minute ?? 0).padStart(2, '0')}`;
        }
        return '';
    };

    return (
        <div className="flex flex-col h-screen bg-zinc-50 dark:bg-zinc-950 overflow-hidden relative">
            {/* 上部固定ヘッダー */}
            <header className="h-16 w-full bg-white dark:bg-zinc-950 border-b border-zinc-200 dark:border-zinc-800 flex items-center px-4 z-30 shrink-0">
                {/* ハンバーガーアイコン */}
                <div
                    ref={buttonRef}
                    className="cursor-pointer p-2 z-50 flex items-center justify-center rounded-md hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors"
                    onClick={() => setIsSidebarOpen(!isSidebarOpen)}
                >
                    <div className="flex flex-col justify-between w-6 h-4">
                        <motion.div
                            animate={{ rotate: isSidebarOpen ? 45 : 0, y: isSidebarOpen ? 7 : 0 }}
                            transition={{ type: 'spring', stiffness: 200, damping: 20 }}
                            className="h-[2px] w-full bg-zinc-900 dark:bg-zinc-100 rounded-full origin-center"
                        />
                        <motion.div
                            animate={{ opacity: isSidebarOpen ? 0 : 1 }}
                            transition={{ type: 'spring', stiffness: 200, damping: 20 }}
                            className="h-[2px] w-full bg-zinc-900 dark:bg-zinc-100 rounded-full"
                        />
                        <motion.div
                            animate={{ rotate: isSidebarOpen ? -45 : 0, y: isSidebarOpen ? -7 : 0 }}
                            transition={{ type: 'spring', stiffness: 200, damping: 20 }}
                            className="h-[2px] w-full bg-zinc-900 dark:bg-zinc-100 rounded-full origin-center"
                        />
                    </div>
                </div>

                {/* アプリタイトル */}
                <h1 className="ml-4 text-xl font-bold flex items-center gap-2 text-zinc-900 dark:text-zinc-50">
                    <Video className="text-blue-600 h-6 w-6" /> MeetingRec
                </h1>

                {/* 録画中インジケーター */}
                {recordingStatus === 'recording' && (
                    <div className="ml-auto flex items-center gap-2">
                        <span className="flex h-3 w-3">
                            <span className="animate-ping absolute inline-flex h-3 w-3 rounded-full bg-red-400 opacity-75"></span>
                            <span className="relative inline-flex rounded-full h-3 w-3 bg-red-500"></span>
                        </span>
                        <span className="text-sm font-mono text-red-600 dark:text-red-400">
                            REC {formatElapsedTime(elapsedSeconds)}
                        </span>
                    </div>
                )}
            </header>

            {/* スライドオーバー型サイドバー */}
            <AnimatePresence>
                {isSidebarOpen && (
                    <>
                        <motion.div
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            className="fixed inset-0 bg-black/20 dark:bg-black/40 z-40"
                        />
                        <motion.aside
                            ref={menuRef}
                            initial={{ x: '-100%' }}
                            animate={{ x: '0%' }}
                            exit={{ x: '-100%' }}
                            transition={{ type: 'tween', duration: 0.3, ease: 'easeInOut' }}
                            className="fixed top-16 left-0 h-[calc(100vh-4rem)] w-64 bg-white dark:bg-zinc-950 border-r border-zinc-200 dark:border-zinc-800 shadow-2xl z-50 flex flex-col"
                        >
                            <nav className="flex-1 px-4 py-6 space-y-2">
                                <Button
                                    variant={activeTab === 'record' ? 'secondary' : 'ghost'}
                                    className="w-full justify-start text-base cursor-pointer"
                                    onClick={() => { setActiveTab('record'); setIsSidebarOpen(false); }}
                                >
                                    <Video className="mr-3 h-5 w-5" /> 録画メイン
                                </Button>
                                <Button
                                    variant={activeTab === 'schedule' ? 'secondary' : 'ghost'}
                                    className="w-full justify-start text-base cursor-pointer"
                                    onClick={() => { setActiveTab('schedule'); setIsSidebarOpen(false); }}
                                >
                                    <Calendar className="mr-3 h-5 w-5" /> スケジュール設定
                                </Button>
                                <Button
                                    variant={activeTab === 'settings' ? 'secondary' : 'ghost'}
                                    className="w-full justify-start text-base cursor-pointer"
                                    onClick={() => { setActiveTab('settings'); setIsSidebarOpen(false); }}
                                >
                                    <Settings className="mr-3 h-5 w-5" /> 設定・アカウント
                                </Button>
                            </nav>

                            <div className="p-4 border-t border-zinc-200 dark:border-zinc-800 flex flex-col gap-3">
                                <div className="flex items-center px-2">
                                    <Avatar className="h-10 w-10 shrink-0">
                                        <AvatarImage src="" />
                                        <AvatarFallback className="bg-blue-100 text-blue-700">
                                            <User className="h-5 w-5" />
                                        </AvatarFallback>
                                    </Avatar>
                                    <div className="flex flex-col ml-3">
                                        <span className="text-sm font-medium text-zinc-900 dark:text-zinc-100">
                                            {authStatus.user_email || 'ユーザー'}
                                        </span>
                                        <span className="text-[11px] text-zinc-500">
                                            {authStatus.is_authenticated ? 'Google 連携済み' : '未連携'}
                                        </span>
                                    </div>
                                </div>
                                <Button
                                    variant="ghost"
                                    className="cursor-pointer w-full justify-start text-red-600 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-950/30"
                                    onClick={handleLogout}
                                >
                                    <LogOut className="mr-3 h-5 w-5" /> ログアウト
                                </Button>
                            </div>
                        </motion.aside>
                    </>
                )}
            </AnimatePresence>

            {/* メインコンテンツ */}
            <main className="flex-1 overflow-y-auto p-4 md:p-8">
                <div className="max-w-4xl mx-auto space-y-8 pb-12">
                    {/* --- 録画タブ --- */}
                    {activeTab === 'record' && (
                        <motion.div
                            initial={{ opacity: 0, y: 10 }}
                            animate={{ opacity: 1, y: 0 }}
                            className="space-y-6"
                        >
                            <div>
                                <h2 className="text-2xl md:text-3xl font-bold tracking-tight">
                                    手動録画
                                </h2>
                                <p className="text-sm md:text-base text-zinc-500 mt-2">
                                    画面と音声をキャプチャし、終了後にGoogle Driveへ自動保存します。
                                </p>
                            </div>

                            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                                {/* 録画コントローラー */}
                                <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                                    <CardHeader>
                                        <CardTitle className="text-lg">録画コントロール</CardTitle>
                                    </CardHeader>
                                    <CardContent className="flex flex-col items-center justify-center py-8">
                                        <div className="relative">
                                            {recordingStatus === 'recording' && (
                                                <span className="absolute -top-2 -right-2 flex h-4 w-4">
                                                    <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-red-400 opacity-75"></span>
                                                    <span className="relative inline-flex rounded-full h-4 w-4 bg-red-500"></span>
                                                </span>
                                            )}
                                            <Button
                                                size="lg"
                                                className={`h-24 w-24 rounded-full shadow-lg cursor-pointer transition-all duration-300 ${
                                                    recordingStatus === 'recording'
                                                        ? 'bg-zinc-900 hover:bg-zinc-800 dark:bg-white dark:hover:bg-zinc-200 text-red-500'
                                                        : 'bg-blue-600 hover:bg-blue-700 text-white'
                                                }`}
                                                onClick={handleToggleRecording}
                                                disabled={recordingStatus === 'stopping'}
                                            >
                                                {recordingStatus === 'recording' ? (
                                                    <Square className="h-10 w-10 fill-current" />
                                                ) : recordingStatus === 'stopping' ? (
                                                    <Loader2 className="h-10 w-10 animate-spin" />
                                                ) : (
                                                    <Play className="h-10 w-10 fill-current ml-2" />
                                                )}
                                            </Button>
                                        </div>
                                        <p className="mt-6 text-sm font-medium text-zinc-600 dark:text-zinc-400">
                                            {recordingStatus === 'recording'
                                                ? `録画中... (${formatElapsedTime(elapsedSeconds)})`
                                                : recordingStatus === 'stopping'
                                                    ? '停止処理中...'
                                                    : 'ボタンを押して録画開始'}
                                        </p>
                                    </CardContent>
                                </Card>

                                {/* 入力ソース設定 */}
                                <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                                    <CardHeader>
                                        <CardTitle className="text-lg">入力ソース</CardTitle>
                                    </CardHeader>
                                    <CardContent className="space-y-6">
                                        <div className="flex items-center justify-between">
                                            <div className="flex items-center space-x-3">
                                                <Monitor className="h-5 w-5 text-zinc-500" />
                                                <Label className="text-sm md:text-base cursor-pointer">
                                                    画面キャプチャ
                                                </Label>
                                            </div>
                                            <Badge variant="outline" className="bg-blue-50 text-blue-700 border-blue-200">
                                                {recordingConfig.resolution}
                                            </Badge>
                                        </div>
                                        <div className="flex flex-col space-y-3">
                                            <div className="flex items-center justify-between">
                                                <div className="flex items-center space-x-3">
                                                    <HardDrive className="h-5 w-5 text-zinc-500" />
                                                    <div className="space-y-0.5">
                                                        <Label className="text-sm md:text-base cursor-pointer">
                                                            システム音声 (相手の声)
                                                        </Label>
                                                        <p className="text-xs text-zinc-500">仮想ステレオミキサー等</p>
                                                    </div>
                                                </div>
                                                <Switch
                                                    checked={recordingConfig.capture_system_audio}
                                                    onCheckedChange={(checked) =>
                                                        handleSaveRecordingConfig({
                                                            ...recordingConfig,
                                                            capture_system_audio: checked,
                                                        })
                                                    }
                                                />
                                            </div>
                                            {recordingConfig.capture_system_audio && (
                                                <div className="pl-8 col-span-2">
                                                    <Select
                                                        value={recordingConfig.audio_device || ''}
                                                        onValueChange={(val) =>
                                                            handleSaveRecordingConfig({ ...recordingConfig, audio_device: val })
                                                        }
                                                    >
                                                        <SelectTrigger className="w-full text-xs h-8">
                                                            <SelectValue placeholder="デバイスを選択 (デフォルト: virtual-audio-capturer)" />
                                                        </SelectTrigger>
                                                        <SelectContent>
                                                            <SelectItem value="virtual-audio-capturer">virtual-audio-capturer (デフォルト規定値)</SelectItem>
                                                            {audioDevices.map(dev => (
                                                                <SelectItem key={dev} value={dev}>{dev}</SelectItem>
                                                            ))}
                                                        </SelectContent>
                                                    </Select>
                                                </div>
                                            )}
                                        </div>

                                        <div className="flex flex-col space-y-3">
                                            <div className="flex items-center justify-between">
                                                <div className="flex items-center space-x-3">
                                                    <Mic className="h-5 w-5 text-zinc-500" />
                                                    <div className="space-y-0.5">
                                                        <Label className="text-sm md:text-base cursor-pointer">
                                                            マイク (自分の声)
                                                        </Label>
                                                    </div>
                                                </div>
                                                <Switch
                                                    checked={recordingConfig.capture_mic}
                                                    onCheckedChange={(checked) =>
                                                        handleSaveRecordingConfig({
                                                            ...recordingConfig,
                                                            capture_mic: checked,
                                                        })
                                                    }
                                                />
                                            </div>
                                            {recordingConfig.capture_mic && (
                                                <div className="pl-8 col-span-2">
                                                    <Select
                                                        value={recordingConfig.mic_device || ''}
                                                        onValueChange={(val) =>
                                                            handleSaveRecordingConfig({ ...recordingConfig, mic_device: val })
                                                        }
                                                    >
                                                        <SelectTrigger className="w-full text-xs h-8">
                                                            <SelectValue placeholder="デバイスを選択 (デフォルト: Microphone)" />
                                                        </SelectTrigger>
                                                        <SelectContent>
                                                            <SelectItem value="Microphone">Microphone (デフォルト規定値)</SelectItem>
                                                            {audioDevices.map(dev => (
                                                                <SelectItem key={`mic_${dev}`} value={dev}>{dev}</SelectItem>
                                                            ))}
                                                        </SelectContent>
                                                    </Select>
                                                </div>
                                            )}
                                        </div>
                                    </CardContent>
                                </Card>
                            </div>
                        </motion.div>
                    )}

                    {/* --- スケジュールタブ --- */}
                    {activeTab === 'schedule' && (
                        <motion.div
                            initial={{ opacity: 0, y: 10 }}
                            animate={{ opacity: 1, y: 0 }}
                            className="space-y-6"
                        >
                            <div className="flex flex-col sm:flex-row sm:justify-between sm:items-end gap-4">
                                <div>
                                    <h2 className="text-2xl md:text-3xl font-bold tracking-tight">
                                        スケジュール＆自動参加
                                    </h2>
                                    <p className="text-sm md:text-base text-zinc-500 mt-2">
                                        指定日時に自動で会議URLを開き、録画を開始します。
                                    </p>
                                </div>
                                <Button
                                    className="cursor-pointer bg-blue-600 hover:bg-blue-700 text-white shrink-0"
                                    onClick={() => {
                                        setEditingSchedule(null);
                                        setShowScheduleDialog(true);
                                    }}
                                >
                                    <Plus className="mr-2 h-4 w-4" /> 新規スケジュール
                                </Button>
                            </div>

                            <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                                <CardContent className="p-0">
                                    {schedules.length === 0 ? (
                                        <div className="p-8 text-center text-zinc-400">
                                            <Calendar className="h-12 w-12 mx-auto mb-4 opacity-50" />
                                            <p className="text-base font-medium">スケジュールがありません</p>
                                            <p className="text-sm mt-1">
                                                「新規スケジュール」ボタンから追加してください
                                            </p>
                                        </div>
                                    ) : (
                                        <div className="divide-y divide-zinc-200 dark:divide-zinc-800">
                                            {schedules.map((schedule) => (
                                                <div
                                                    key={schedule.id}
                                                    className="p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4 hover:bg-zinc-50 dark:hover:bg-zinc-900/50 transition-colors"
                                                >
                                                    <div className="flex items-start space-x-4">
                                                        <div className="p-2 bg-blue-100 dark:bg-blue-900/30 rounded-lg shrink-0">
                                                            <Clock className="h-5 w-5 text-blue-600 dark:text-blue-400" />
                                                        </div>
                                                        <div>
                                                            <h4 className="font-medium text-zinc-900 dark:text-zinc-100">
                                                                {schedule.name}
                                                            </h4>
                                                            <div className="flex items-center space-x-3 mt-1">
                                                                <span className="text-xs text-zinc-500">
                                                                    {formatScheduleTime(schedule)}
                                                                </span>
                                                                <span className="text-xs text-zinc-400">
                                                                    {schedule.duration_minutes
                                                                        ? `${schedule.duration_minutes}分`
                                                                        : '手動停止'}
                                                                </span>
                                                            </div>
                                                            <a
                                                                href="#"
                                                                className="text-xs text-blue-500 hover:underline flex items-center mt-1"
                                                                title={schedule.url}
                                                            >
                                                                <ExternalLink className="h-3 w-3 mr-1" />
                                                                {schedule.url.length > 35
                                                                    ? schedule.url.slice(0, 35) + '...'
                                                                    : schedule.url}
                                                            </a>
                                                        </div>
                                                    </div>
                                                    <div className="flex items-center space-x-3 ml-12 sm:ml-0">
                                                        <Badge variant={schedule.active ? 'default' : 'secondary'}>
                                                            {schedule.active ? '有効' : '無効'}
                                                        </Badge>
                                                        <Switch
                                                            checked={schedule.active}
                                                            onCheckedChange={(checked) =>
                                                                handleToggleSchedule(schedule.id, checked)
                                                            }
                                                        />
                                                        <Button
                                                            variant="ghost"
                                                            size="icon"
                                                            className="h-8 w-8 cursor-pointer text-zinc-400 hover:text-red-600"
                                                            onClick={() => handleDeleteSchedule(schedule.id)}
                                                        >
                                                            <Trash2 className="h-4 w-4" />
                                                        </Button>
                                                    </div>
                                                </div>
                                            ))}
                                        </div>
                                    )}
                                </CardContent>
                            </Card>

                            {/* スケジュール追加ダイアログ */}
                            <ScheduleDialog
                                open={showScheduleDialog}
                                onOpenChange={setShowScheduleDialog}
                                onSave={handleAddSchedule}
                                editSchedule={editingSchedule}
                            />
                        </motion.div>
                    )}

                    {/* --- 設定タブ --- */}
                    {activeTab === 'settings' && (
                        <motion.div
                            initial={{ opacity: 0, y: 10 }}
                            animate={{ opacity: 1, y: 0 }}
                            className="space-y-6"
                        >
                            <div>
                                <h2 className="text-2xl md:text-3xl font-bold tracking-tight">
                                    設定・アカウント
                                </h2>
                                <p className="text-sm md:text-base text-zinc-500 mt-2">
                                    録画品質や連携サービスの設定を行います。
                                </p>
                            </div>

                            <div className="grid grid-cols-1 gap-6">
                                {/* Google Drive 連携設定 */}
                                <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                                    <CardHeader>
                                        <div className="flex items-center justify-between">
                                            <CardTitle className="text-lg flex items-center">
                                                <CloudUpload className="mr-2 h-5 w-5 text-blue-600" />
                                                Google Drive 連携
                                            </CardTitle>
                                            {authStatus.is_authenticated ? (
                                                <Badge className="bg-green-100 text-green-700 border-green-200">
                                                    <CheckCircle className="h-3 w-3 mr-1" />
                                                    連携済み
                                                </Badge>
                                            ) : (
                                                <Badge variant="secondary">
                                                    <AlertCircle className="h-3 w-3 mr-1" />
                                                    未連携
                                                </Badge>
                                            )}
                                        </div>
                                        {authStatus.user_email && (
                                            <CardDescription>{authStatus.user_email}</CardDescription>
                                        )}
                                    </CardHeader>
                                    <CardContent className="space-y-4">
                                        {!authStatus.is_authenticated && (
                                            <Button
                                                className="w-full cursor-pointer bg-blue-600 hover:bg-blue-700 text-white"
                                                onClick={handleGoogleAuth}
                                                disabled={isAuthenticating}
                                            >
                                                {isAuthenticating ? (
                                                    <>
                                                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                                                        認証中...
                                                    </>
                                                ) : (
                                                    'Google アカウントで認証'
                                                )}
                                            </Button>
                                        )}
                                        <div className="grid gap-2">
                                            <Label>保存先フォルダ名</Label>
                                            <Input
                                                value={driveConfig.folder_name}
                                                onChange={(e) =>
                                                    setDriveConfigState({
                                                        ...driveConfig,
                                                        folder_name: e.target.value,
                                                    })
                                                }
                                                onBlur={() => handleSaveDriveConfig(driveConfig)}
                                            />
                                        </div>
                                        <div className="flex items-center justify-between pt-2">
                                            <Label className="text-sm font-normal text-zinc-600 dark:text-zinc-400 cursor-pointer">
                                                アップロード完了後にローカルファイルを削除する
                                            </Label>
                                            <Switch
                                                checked={driveConfig.delete_after_upload}
                                                onCheckedChange={(checked) =>
                                                    handleSaveDriveConfig({
                                                        ...driveConfig,
                                                        delete_after_upload: checked,
                                                    })
                                                }
                                            />
                                        </div>
                                    </CardContent>
                                </Card>

                                {/* 録画品質設定 */}
                                <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                                    <CardHeader>
                                        <CardTitle className="text-lg">録画品質 (FFmpeg)</CardTitle>
                                    </CardHeader>
                                    <CardContent className="space-y-4">
                                        <div className="grid gap-2">
                                            <Label>解像度</Label>
                                            <Select
                                                value={recordingConfig.resolution}
                                                onValueChange={(value) =>
                                                    handleSaveRecordingConfig({
                                                        ...recordingConfig,
                                                        resolution: value,
                                                    })
                                                }
                                            >
                                                <SelectTrigger>
                                                    <SelectValue placeholder="解像度を選択" />
                                                </SelectTrigger>
                                                <SelectContent>
                                                    <SelectItem value="720p">720p (容量節約)</SelectItem>
                                                    <SelectItem value="1080p">1080p (標準)</SelectItem>
                                                    <SelectItem value="4k">4K (高画質)</SelectItem>
                                                </SelectContent>
                                            </Select>
                                        </div>
                                        <div className="grid gap-2">
                                            <Label>フレームレート</Label>
                                            <Select
                                                value={String(recordingConfig.framerate)}
                                                onValueChange={(value) =>
                                                    handleSaveRecordingConfig({
                                                        ...recordingConfig,
                                                        framerate: parseInt(value),
                                                    })
                                                }
                                            >
                                                <SelectTrigger>
                                                    <SelectValue placeholder="FPSを選択" />
                                                </SelectTrigger>
                                                <SelectContent>
                                                    <SelectItem value="15">15 fps (資料メイン)</SelectItem>
                                                    <SelectItem value="30">30 fps (標準)</SelectItem>
                                                    <SelectItem value="60">60 fps (滑らか)</SelectItem>
                                                </SelectContent>
                                            </Select>
                                        </div>
                                    </CardContent>
                                </Card>
                            </div>
                        </motion.div>
                    )}
                </div>
            </main>
        </div>
    );
}
