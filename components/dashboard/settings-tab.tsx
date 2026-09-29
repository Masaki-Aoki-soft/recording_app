'use client';

import { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart';
import { AlertCircle, AlertTriangle, CheckCircle, CloudUpload, Info, Loader2, Power, Video } from 'lucide-react';
import toast from 'react-hot-toast';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
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
    connectGoogleDrive,
    disconnectGoogleDrive,
    errorMessage,
    getDriveAuthStatus,
    getDriveConfig,
    getGeneralSettings,
    getRecordingConfig,
    isTauri,
    saveGeneralSettings,
    saveRecordingConfig,
    setDriveConfig,
    type DriveAuthStatus,
    type DriveConfig,
    type GeneralSettings,
    type RecordingConfig,
} from '@/lib/tauri';

export default function SettingsTab() {
    const [general, setGeneral] = useState<GeneralSettings | null>(null);
    const [recording, setRecording] = useState<RecordingConfig | null>(null);
    const [drive, setDrive] = useState<DriveConfig | null>(null);
    const [driveAuth, setDriveAuth] = useState<DriveAuthStatus>({ connected: false, email: null });
    const [autostart, setAutostart] = useState(false);
    const [connecting, setConnecting] = useState(false);

    useEffect(() => {
        if (!isTauri()) return;
        getGeneralSettings().then(setGeneral).catch(console.error);
        getRecordingConfig().then(setRecording).catch(console.error);
        getDriveConfig().then(setDrive).catch(console.error);
        getDriveAuthStatus().then(setDriveAuth).catch(console.error);
        isEnabled().then(setAutostart).catch(console.error);
    }, []);

    const persist = async (fn: () => Promise<void>) => {
        try {
            await fn();
            toast.success('設定を保存しました', { id: 'settings-saved' });
        } catch (err) {
            toast.error(`設定を保存できませんでした: ${errorMessage(err)}`);
        }
    };

    const updateGeneral = (patch: Partial<GeneralSettings>, save = true) => {
        if (!general) return;
        const next = { ...general, ...patch };
        setGeneral(next);
        if (save) persist(() => saveGeneralSettings(next));
    };

    const updateRecording = (patch: Partial<RecordingConfig>) => {
        if (!recording) return;
        const next = { ...recording, ...patch };
        setRecording(next);
        persist(() => saveRecordingConfig(next));
    };

    const updateDrive = (patch: Partial<DriveConfig>, save = true) => {
        if (!drive) return;
        const next = { ...drive, ...patch };
        setDrive(next);
        if (save) persist(() => setDriveConfig(next));
    };

    const handleConnect = async () => {
        setConnecting(true);
        try {
            await connectGoogleDrive();
            setDriveAuth(await getDriveAuthStatus());
            toast.success('Google Drive と連携しました');
        } catch (err) {
            toast.error(`連携に失敗しました: ${errorMessage(err)}`);
        } finally {
            setConnecting(false);
        }
    };

    const handleDisconnect = async () => {
        if (!window.confirm('Google Drive との連携を解除しますか？')) return;
        try {
            await disconnectGoogleDrive();
            setDriveAuth({ connected: false, email: null });
            toast.success('連携を解除しました');
        } catch (err) {
            toast.error(errorMessage(err));
        }
    };

    const handleAutostart = async (checked: boolean) => {
        try {
            if (checked) await enable();
            else await disable();
            setAutostart(await isEnabled());
        } catch (err) {
            toast.error(`自動起動を変更できませんでした: ${errorMessage(err)}`);
        }
    };

    return (
        <motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} className="space-y-6">
            <div>
                <h2 className="text-2xl md:text-3xl font-bold tracking-tight">設定・アカウント</h2>
                <p className="text-sm md:text-base text-zinc-500 mt-2">
                    Zoom 参加・録画品質・保存先の設定を行います。
                </p>
            </div>

            {/* 録画の同意 */}
            <div className="flex gap-3 rounded-lg border border-amber-200 bg-amber-50 dark:bg-amber-950/30 dark:border-amber-900 p-4 text-sm text-amber-900 dark:text-amber-200">
                <AlertTriangle className="h-5 w-5 shrink-0" />
                <p>
                    会議を録画する際は、事前に主催者・参加者の同意を得てください。本アプリは画面キャプチャで録画するため、Zoom
                    の録画通知は他の参加者に表示されません。
                </p>
            </div>

            {/* Zoom 参加設定 */}
            <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                <CardHeader>
                    <CardTitle className="text-lg flex items-center">
                        <Video className="mr-2 h-5 w-5 text-blue-600" /> Zoom 参加設定
                    </CardTitle>
                </CardHeader>
                <CardContent className="space-y-4">
                    <div className="grid gap-2">
                        <Label htmlFor="zoom-name">参加時の表示名</Label>
                        <Input
                            id="zoom-name"
                            value={general?.zoom_display_name ?? ''}
                            disabled={!general}
                            onChange={(e) => updateGeneral({ zoom_display_name: e.target.value }, false)}
                            onBlur={() => general && persist(() => saveGeneralSettings(general))}
                        />
                    </div>
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                        <div className="grid gap-2">
                            <Label>Zoom を起動するタイミング</Label>
                            <Select
                                value={String(general?.lead_seconds ?? 60)}
                                disabled={!general}
                                onValueChange={(v) => updateGeneral({ lead_seconds: parseInt(v) })}
                            >
                                <SelectTrigger>
                                    <SelectValue />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="0">開始時刻ちょうど</SelectItem>
                                    <SelectItem value="30">30 秒前</SelectItem>
                                    <SelectItem value="60">1 分前</SelectItem>
                                    <SelectItem value="120">2 分前</SelectItem>
                                    <SelectItem value="300">5 分前</SelectItem>
                                </SelectContent>
                            </Select>
                        </div>
                        <div className="grid gap-2">
                            <Label>会議開始を待つ最大時間</Label>
                            <Select
                                value={String(general?.wait_timeout_minutes ?? 30)}
                                disabled={!general}
                                onValueChange={(v) => updateGeneral({ wait_timeout_minutes: parseInt(v) })}
                            >
                                <SelectTrigger>
                                    <SelectValue />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="10">10 分</SelectItem>
                                    <SelectItem value="30">30 分</SelectItem>
                                    <SelectItem value="60">60 分</SelectItem>
                                    <SelectItem value="120">120 分</SelectItem>
                                </SelectContent>
                            </Select>
                        </div>
                    </div>
                    <div className="flex gap-3 rounded-md bg-zinc-100 dark:bg-zinc-900 p-3 text-xs text-zinc-600 dark:text-zinc-400">
                        <Info className="h-4 w-4 shrink-0" />
                        <div className="space-y-1">
                            <p className="font-medium">Zoom アプリ側で次の設定を有効にしてください（設定 → オーディオ / ビデオ）</p>
                            <ul className="list-disc pl-4 space-y-0.5">
                                <li>「ミーティングへの参加時に、コンピューターでオーディオに参加」</li>
                                <li>「ミーティングに参加する際、マイクをミュートに設定」</li>
                                <li>「ミーティングに参加する際、ビデオをオフにする」</li>
                                <li>「ビデオプレビューダイアログを表示」をオフ</li>
                            </ul>
                        </div>
                    </div>
                </CardContent>
            </Card>

            {/* Google Drive 連携設定 */}
            <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                <CardHeader>
                    <div className="flex items-center justify-between">
                        <CardTitle className="text-lg flex items-center">
                            <CloudUpload className="mr-2 h-5 w-5 text-blue-600" />
                            Google Drive 連携
                        </CardTitle>
                        {driveAuth.connected ? (
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
                    {driveAuth.email && <CardDescription>{driveAuth.email}</CardDescription>}
                </CardHeader>
                <CardContent className="space-y-4">
                    {driveAuth.connected ? (
                        <Button variant="outline" className="w-full cursor-pointer" onClick={handleDisconnect}>
                            連携を解除
                        </Button>
                    ) : (
                        <Button
                            className="w-full cursor-pointer bg-blue-600 hover:bg-blue-700 text-white"
                            onClick={handleConnect}
                            disabled={connecting || !isTauri()}
                        >
                            {connecting ? (
                                <>
                                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                                    ブラウザで認証中...
                                </>
                            ) : (
                                'Google アカウントで連携'
                            )}
                        </Button>
                    )}
                    <div className="grid gap-2">
                        <Label htmlFor="drive-folder">保存先フォルダ名</Label>
                        <Input
                            id="drive-folder"
                            value={drive?.folder_name ?? ''}
                            disabled={!drive}
                            onChange={(e) => updateDrive({ folder_name: e.target.value }, false)}
                            onBlur={() => drive && persist(() => setDriveConfig(drive))}
                        />
                    </div>
                    <div className="flex items-center justify-between pt-2">
                        <Label htmlFor="auto-upload" className="text-sm font-normal text-zinc-600 dark:text-zinc-400 cursor-pointer">
                            録画完了後に自動でアップロードする
                        </Label>
                        <Switch
                            id="auto-upload"
                            checked={drive?.auto_upload ?? false}
                            disabled={!drive}
                            onCheckedChange={(checked) => updateDrive({ auto_upload: checked })}
                        />
                    </div>
                    <div className="flex items-center justify-between">
                        <Label htmlFor="delete-after" className="text-sm font-normal text-zinc-600 dark:text-zinc-400 cursor-pointer">
                            アップロード完了後にローカルファイルを削除する
                        </Label>
                        <Switch
                            id="delete-after"
                            checked={drive?.delete_after_upload ?? false}
                            disabled={!drive}
                            onCheckedChange={(checked) => updateDrive({ delete_after_upload: checked })}
                        />
                    </div>
                </CardContent>
            </Card>

            {/* 録画品質設定 */}
            <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                <CardHeader>
                    <CardTitle className="text-lg">録画品質</CardTitle>
                    <CardDescription>キャプチャが指定の解像度より大きい場合に縮小します</CardDescription>
                </CardHeader>
                <CardContent className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div className="grid gap-2">
                        <Label>最大解像度</Label>
                        <Select
                            value={recording?.resolution ?? '1080p'}
                            disabled={!recording}
                            onValueChange={(value) => updateRecording({ resolution: value })}
                        >
                            <SelectTrigger>
                                <SelectValue placeholder="解像度を選択" />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="720p">720p（容量節約）</SelectItem>
                                <SelectItem value="1080p">1080p（標準）</SelectItem>
                                <SelectItem value="4k">4K（高画質）</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>
                    <div className="grid gap-2">
                        <Label>フレームレート</Label>
                        <Select
                            value={String(recording?.framerate ?? 30)}
                            disabled={!recording}
                            onValueChange={(value) => updateRecording({ framerate: parseInt(value) })}
                        >
                            <SelectTrigger>
                                <SelectValue placeholder="FPSを選択" />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="15">15 fps（資料メイン）</SelectItem>
                                <SelectItem value="30">30 fps（標準）</SelectItem>
                                <SelectItem value="60">60 fps（滑らか）</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>
                </CardContent>
            </Card>

            {/* 起動設定 */}
            <Card className="border-zinc-200 dark:border-zinc-800 shadow-sm">
                <CardHeader>
                    <CardTitle className="text-lg flex items-center">
                        <Power className="mr-2 h-5 w-5 text-blue-600" /> 起動設定
                    </CardTitle>
                </CardHeader>
                <CardContent>
                    <div className="flex items-center justify-between gap-4">
                        <div className="space-y-0.5">
                            <Label htmlFor="autostart" className="cursor-pointer">
                                Windows のログイン時に自動で起動する
                            </Label>
                            <p className="text-xs text-zinc-500">
                                タスクトレイに常駐し、スケジュールの時刻になると自動で録画します
                            </p>
                        </div>
                        <Switch
                            id="autostart"
                            checked={autostart}
                            disabled={!isTauri()}
                            onCheckedChange={handleAutostart}
                        />
                    </div>
                </CardContent>
            </Card>
        </motion.div>
    );
}
