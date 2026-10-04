'use client';

import { useCallback, useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import { Calendar, Clock, Edit, ExternalLink, PlayCircle, Plus, Trash2 } from 'lucide-react';
import toast from 'react-hot-toast';

import ScheduleDialog from '@/components/schedule-dialog';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import {
    addSchedule,
    deleteSchedule,
    errorMessage,
    isTauri,
    listSchedules,
    onSchedulesChanged,
    runScheduleNow,
    toggleSchedule,
    updateSchedule,
    type Schedule,
    type ScheduleInput,
    type SessionStatus,
} from '@/lib/tauri';

const DAYS = ['日', '月', '火', '水', '木', '金', '土'];

function formatScheduleTime(schedule: Schedule): string {
    const st = schedule.schedule_type;
    if (st.type === 'Once' && st.datetime) {
        return new Date(st.datetime).toLocaleString('ja-JP', {
            year: 'numeric',
            month: 'numeric',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit',
        });
    }
    if (st.type === 'Weekly') {
        return `毎週${DAYS[st.day_of_week ?? 0]}曜日 ${String(st.hour ?? 0).padStart(2, '0')}:${String(
            st.minute ?? 0,
        ).padStart(2, '0')}`;
    }
    return '';
}

function formatNextRun(nextRun: string | null | undefined): string | null {
    if (!nextRun) return null;
    return new Date(nextRun).toLocaleString('ja-JP', {
        month: 'numeric',
        day: 'numeric',
        weekday: 'short',
        hour: '2-digit',
        minute: '2-digit',
    });
}

export default function ScheduleTab({ session }: { session: SessionStatus }) {
    const [schedules, setSchedules] = useState<Schedule[]>([]);
    const [dialogOpen, setDialogOpen] = useState(false);
    const [editing, setEditing] = useState<Schedule | null>(null);

    const refresh = useCallback(async () => {
        if (!isTauri()) return;
        try {
            setSchedules(await listSchedules());
        } catch (err) {
            console.error('Failed to load schedules:', err);
        }
    }, []);

    useEffect(() => {
        if (!isTauri()) return;
        listSchedules()
            .then(setSchedules)
            .catch((err) => console.error('Failed to load schedules:', err));
        // 単発スケジュールの自動無効化などバックエンド側の変更を反映
        let unlisten: (() => void) | undefined;
        onSchedulesChanged(refresh).then((fn) => (unlisten = fn));
        return () => unlisten?.();
    }, [refresh]);

    const handleSave = async (input: ScheduleInput) => {
        try {
            if (input.id) {
                await updateSchedule({ ...input, id: input.id });
                toast.success('スケジュールを更新しました');
            } else {
                await addSchedule(input);
                toast.success('スケジュールを追加しました');
            }
            await refresh();
            return true;
        } catch (err) {
            toast.error(errorMessage(err));
            return false;
        }
    };

    const handleDelete = async (schedule: Schedule) => {
        if (!window.confirm(`「${schedule.name}」を削除しますか？`)) return;
        try {
            await deleteSchedule(schedule.id);
            toast.success('スケジュールを削除しました');
            await refresh();
        } catch (err) {
            toast.error(errorMessage(err));
        }
    };

    const handleToggle = async (id: string, active: boolean) => {
        try {
            await toggleSchedule(id, active);
            await refresh();
        } catch (err) {
            toast.error(errorMessage(err));
        }
    };

    const handleRunNow = async (schedule: Schedule) => {
        try {
            await runScheduleNow(schedule.id);
            toast.success(`「${schedule.name}」に参加して録画を開始します`);
        } catch (err) {
            toast.error(errorMessage(err));
        }
    };

    const sessionBusy = session.state !== 'idle' && session.state !== 'error';

    return (
        <motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} className="space-y-6">
            <div className="flex flex-col sm:flex-row sm:justify-between sm:items-end gap-4">
                <div>
                    <h2 className="text-2xl md:text-3xl font-bold tracking-tight">スケジュール＆自動参加</h2>
                    <p className="text-sm md:text-base text-zinc-500 mt-2">
                        指定日時に Zoom アプリで会議に参加し、会議ウィンドウを自動で録画します。
                    </p>
                </div>
                <Button
                    className="cursor-pointer bg-blue-600 hover:bg-blue-700 text-white shrink-0"
                    disabled={!isTauri()}
                    onClick={() => {
                        setEditing(null);
                        setDialogOpen(true);
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
                            <p className="text-sm mt-1">「新規スケジュール」ボタンから追加してください</p>
                        </div>
                    ) : (
                        <div className="divide-y divide-zinc-200 dark:divide-zinc-800">
                            {schedules.map((schedule) => {
                                const next = formatNextRun(schedule.next_run);
                                return (
                                    <div
                                        key={schedule.id}
                                        className="p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4 hover:bg-zinc-50 dark:hover:bg-zinc-900/50 transition-colors"
                                    >
                                        <div className="flex items-start space-x-4 min-w-0">
                                            <div className="p-2 bg-blue-100 dark:bg-blue-900/30 rounded-lg shrink-0">
                                                <Clock className="h-5 w-5 text-blue-600 dark:text-blue-400" />
                                            </div>
                                            <div className="min-w-0">
                                                <h4 className="font-medium text-zinc-900 dark:text-zinc-100 truncate">
                                                    {schedule.name}
                                                </h4>
                                                <div className="flex flex-wrap items-center gap-x-3 mt-1">
                                                    <span className="text-xs text-zinc-500">
                                                        {formatScheduleTime(schedule)}
                                                    </span>
                                                    <span className="text-xs text-zinc-400">
                                                        {schedule.duration_minutes
                                                            ? `${schedule.duration_minutes}分`
                                                            : '会議終了まで'}
                                                    </span>
                                                    {next && (
                                                        <span className="text-xs text-blue-600">次回: {next}</span>
                                                    )}
                                                </div>
                                                <p
                                                    className="text-xs text-zinc-400 flex items-center mt-1 truncate"
                                                    title={schedule.url}
                                                >
                                                    <ExternalLink className="h-3 w-3 mr-1 shrink-0" />
                                                    {schedule.url}
                                                </p>
                                            </div>
                                        </div>
                                        <div className="flex items-center space-x-2 ml-12 sm:ml-0 shrink-0">
                                            <Badge variant={schedule.active ? 'default' : 'secondary'}>
                                                {schedule.active ? '有効' : '無効'}
                                            </Badge>
                                            <Switch
                                                checked={schedule.active}
                                                onCheckedChange={(checked) => handleToggle(schedule.id, checked)}
                                            />
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                title="今すぐ参加して録画"
                                                className="h-8 w-8 cursor-pointer text-zinc-400 hover:text-blue-600"
                                                disabled={sessionBusy}
                                                onClick={() => handleRunNow(schedule)}
                                            >
                                                <PlayCircle className="h-4 w-4" />
                                            </Button>
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                title="編集"
                                                className="h-8 w-8 cursor-pointer text-zinc-400 hover:text-zinc-900 dark:hover:text-zinc-100"
                                                onClick={() => {
                                                    setEditing(schedule);
                                                    setDialogOpen(true);
                                                }}
                                            >
                                                <Edit className="h-4 w-4" />
                                            </Button>
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                title="削除"
                                                className="h-8 w-8 cursor-pointer text-zinc-400 hover:text-red-600"
                                                onClick={() => handleDelete(schedule)}
                                            >
                                                <Trash2 className="h-4 w-4" />
                                            </Button>
                                        </div>
                                    </div>
                                );
                            })}
                        </div>
                    )}
                </CardContent>
            </Card>

            <ScheduleDialog
                // 編集対象が変わるたびに再マウントして初期値を反映する
                key={dialogOpen ? (editing?.id ?? 'new') : 'closed'}
                open={dialogOpen}
                onOpenChange={setDialogOpen}
                onSave={handleSave}
                editSchedule={editing}
            />
        </motion.div>
    );
}
