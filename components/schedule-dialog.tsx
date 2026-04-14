'use client';

import { useState } from 'react';
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
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
import type { Schedule, ScheduleType } from '@/lib/tauri';

interface ScheduleDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onSave: (schedule: Omit<Schedule, 'id'> & { id?: string }) => void;
    editSchedule?: Schedule | null;
}

const DAY_NAMES = ['日曜日', '月曜日', '火曜日', '水曜日', '木曜日', '金曜日', '土曜日'];

export default function ScheduleDialog({
    open,
    onOpenChange,
    onSave,
    editSchedule,
}: ScheduleDialogProps) {
    const [name, setName] = useState(editSchedule?.name || '');
    const [url, setUrl] = useState(editSchedule?.url || '');
    const [scheduleMode, setScheduleMode] = useState<'once' | 'weekly'>(
        editSchedule?.schedule_type?.type === 'Weekly' ? 'weekly' : 'once',
    );
    const [dateTime, setDateTime] = useState(
        editSchedule?.schedule_type?.datetime?.slice(0, 16) || '',
    );
    const [dayOfWeek, setDayOfWeek] = useState(
        String(editSchedule?.schedule_type?.day_of_week ?? 1),
    );
    const [hour, setHour] = useState(String(editSchedule?.schedule_type?.hour ?? 10));
    const [minute, setMinute] = useState(String(editSchedule?.schedule_type?.minute ?? 0));
    const [hasDuration, setHasDuration] = useState(editSchedule?.duration_minutes != null);
    const [durationMinutes, setDurationMinutes] = useState(
        String(editSchedule?.duration_minutes ?? 60),
    );

    const handleSave = () => {
        if (!name.trim() || !url.trim()) return;

        let schedule_type: ScheduleType;
        if (scheduleMode === 'once') {
            if (!dateTime) return;
            // ローカル日時を ISO 8601 に変換
            const dt = new Date(dateTime);
            schedule_type = {
                type: 'Once',
                datetime: dt.toISOString(),
            };
        } else {
            schedule_type = {
                type: 'Weekly',
                day_of_week: parseInt(dayOfWeek),
                hour: parseInt(hour),
                minute: parseInt(minute),
            };
        }

        onSave({
            id: editSchedule?.id,
            name: name.trim(),
            url: url.trim(),
            schedule_type,
            active: editSchedule?.active ?? true,
            duration_minutes: hasDuration ? parseInt(durationMinutes) : null,
        });

        onOpenChange(false);
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-[480px] bg-white dark:bg-zinc-900 border-zinc-200 dark:border-zinc-800">
                <DialogHeader>
                    <DialogTitle className="text-xl">
                        {editSchedule ? 'スケジュール編集' : '新規スケジュール'}
                    </DialogTitle>
                    <DialogDescription>
                        会議の自動参加・録画スケジュールを設定します。
                    </DialogDescription>
                </DialogHeader>

                <div className="space-y-5 py-4">
                    {/* 会議名 */}
                    <div className="space-y-2">
                        <Label htmlFor="schedule-name">会議名</Label>
                        <Input
                            id="schedule-name"
                            placeholder="例: 定例ミーティング"
                            value={name}
                            onChange={(e) => setName(e.target.value)}
                        />
                    </div>

                    {/* URL */}
                    <div className="space-y-2">
                        <Label htmlFor="schedule-url">会議URL</Label>
                        <Input
                            id="schedule-url"
                            placeholder="https://zoom.us/j/..."
                            value={url}
                            onChange={(e) => setUrl(e.target.value)}
                        />
                    </div>

                    {/* スケジュール種別 */}
                    <div className="space-y-2">
                        <Label>スケジュール種別</Label>
                        <Select
                            value={scheduleMode}
                            onValueChange={(v) => setScheduleMode(v as 'once' | 'weekly')}
                        >
                            <SelectTrigger>
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="once">単発（特定日時）</SelectItem>
                                <SelectItem value="weekly">毎週繰り返し</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>

                    {/* 単発: 日時選択 */}
                    {scheduleMode === 'once' && (
                        <div className="space-y-2">
                            <Label htmlFor="schedule-datetime">日時</Label>
                            <Input
                                id="schedule-datetime"
                                type="datetime-local"
                                value={dateTime}
                                onChange={(e) => setDateTime(e.target.value)}
                            />
                        </div>
                    )}

                    {/* 毎週: 曜日 + 時刻 */}
                    {scheduleMode === 'weekly' && (
                        <div className="grid grid-cols-3 gap-3">
                            <div className="space-y-2">
                                <Label>曜日</Label>
                                <Select value={dayOfWeek} onValueChange={setDayOfWeek}>
                                    <SelectTrigger>
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {DAY_NAMES.map((day, i) => (
                                            <SelectItem key={i} value={String(i)}>
                                                {day}
                                            </SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            </div>
                            <div className="space-y-2">
                                <Label>時</Label>
                                <Select value={hour} onValueChange={setHour}>
                                    <SelectTrigger>
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {Array.from({ length: 24 }, (_, i) => (
                                            <SelectItem key={i} value={String(i)}>
                                                {String(i).padStart(2, '0')}
                                            </SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            </div>
                            <div className="space-y-2">
                                <Label>分</Label>
                                <Select value={minute} onValueChange={setMinute}>
                                    <SelectTrigger>
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {[0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55].map(
                                            (m) => (
                                                <SelectItem key={m} value={String(m)}>
                                                    {String(m).padStart(2, '0')}
                                                </SelectItem>
                                            ),
                                        )}
                                    </SelectContent>
                                </Select>
                            </div>
                        </div>
                    )}

                    {/* 録画時間 */}
                    <div className="space-y-3">
                        <div className="flex items-center justify-between">
                            <Label className="cursor-pointer">録画時間を指定</Label>
                            <Switch
                                checked={hasDuration}
                                onCheckedChange={setHasDuration}
                            />
                        </div>
                        {hasDuration && (
                            <div className="flex items-center gap-2">
                                <Input
                                    type="number"
                                    min={1}
                                    max={480}
                                    value={durationMinutes}
                                    onChange={(e) => setDurationMinutes(e.target.value)}
                                    className="w-24"
                                />
                                <span className="text-sm text-zinc-500">分後に自動停止</span>
                            </div>
                        )}
                        {!hasDuration && (
                            <p className="text-xs text-zinc-400">
                                手動で停止するまで録画を続けます
                            </p>
                        )}
                    </div>
                </div>

                <DialogFooter>
                    <Button
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                        className="cursor-pointer"
                    >
                        キャンセル
                    </Button>
                    <Button
                        onClick={handleSave}
                        className="cursor-pointer bg-blue-600 hover:bg-blue-700 text-white"
                        disabled={!name.trim() || !url.trim()}
                    >
                        {editSchedule ? '更新' : '追加'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
