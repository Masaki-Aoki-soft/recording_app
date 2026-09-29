'use client';

import { useState } from 'react';
import { Loader2 } from 'lucide-react';

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
import type { Schedule, ScheduleInput, ScheduleType } from '@/lib/tauri';
import { scheduleFormSchema } from '@/lib/validation';

interface ScheduleDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    /** 保存に成功したら true を返す（ダイアログを閉じる） */
    onSave: (schedule: ScheduleInput) => Promise<boolean>;
    editSchedule?: Schedule | null;
}

const DAY_NAMES = ['日曜日', '月曜日', '火曜日', '水曜日', '木曜日', '金曜日', '土曜日'];

/** ISO 8601 文字列を <input type="datetime-local"> 用のローカル時刻文字列に変換 */
function toLocalInputValue(iso: string | undefined): string {
    if (!iso) return '';
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(
        d.getMinutes(),
    )}`;
}

export default function ScheduleDialog({ open, onOpenChange, onSave, editSchedule }: ScheduleDialogProps) {
    const [name, setName] = useState(editSchedule?.name || '');
    const [url, setUrl] = useState(editSchedule?.url || '');
    const [scheduleMode, setScheduleMode] = useState<'once' | 'weekly'>(
        editSchedule?.schedule_type?.type === 'Weekly' ? 'weekly' : 'once',
    );
    const [dateTime, setDateTime] = useState(toLocalInputValue(editSchedule?.schedule_type?.datetime));
    const [dayOfWeek, setDayOfWeek] = useState(String(editSchedule?.schedule_type?.day_of_week ?? 1));
    const [hour, setHour] = useState(String(editSchedule?.schedule_type?.hour ?? 10));
    const [minute, setMinute] = useState(String(editSchedule?.schedule_type?.minute ?? 0));
    const [hasDuration, setHasDuration] = useState(editSchedule?.duration_minutes != null);
    const [durationMinutes, setDurationMinutes] = useState(String(editSchedule?.duration_minutes ?? 60));
    const [errors, setErrors] = useState<Record<string, string>>({});
    const [saving, setSaving] = useState(false);

    const handleSave = async () => {
        const nextErrors: Record<string, string> = {};

        const parsed = scheduleFormSchema.safeParse({ name, url });
        if (!parsed.success) {
            for (const issue of parsed.error.issues) {
                nextErrors[String(issue.path[0])] = issue.message;
            }
        }

        let schedule_type: ScheduleType | null = null;
        if (scheduleMode === 'once') {
            const dt = new Date(dateTime);
            if (!dateTime || Number.isNaN(dt.getTime())) {
                nextErrors.datetime = '日時を入力してください';
            } else if (dt.getTime() < Date.now() && !editSchedule) {
                nextErrors.datetime = '過去の日時は指定できません';
            } else {
                schedule_type = { type: 'Once', datetime: dt.toISOString() };
            }
        } else {
            schedule_type = {
                type: 'Weekly',
                day_of_week: parseInt(dayOfWeek),
                hour: parseInt(hour),
                minute: parseInt(minute),
            };
        }

        const duration = parseInt(durationMinutes);
        if (hasDuration && (!Number.isFinite(duration) || duration < 1 || duration > 720)) {
            nextErrors.duration = '1〜720 分の範囲で指定してください';
        }

        setErrors(nextErrors);
        if (Object.keys(nextErrors).length > 0 || !parsed.success || !schedule_type) return;

        setSaving(true);
        const ok = await onSave({
            id: editSchedule?.id,
            name: parsed.data.name,
            url: parsed.data.url,
            schedule_type,
            active: editSchedule?.active ?? true,
            duration_minutes: hasDuration ? duration : null,
        });
        setSaving(false);
        if (ok) onOpenChange(false);
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-[480px] bg-white dark:bg-zinc-900 border-zinc-200 dark:border-zinc-800">
                <DialogHeader>
                    <DialogTitle className="text-xl">
                        {editSchedule ? 'スケジュール編集' : '新規スケジュール'}
                    </DialogTitle>
                    <DialogDescription>会議の自動参加・録画スケジュールを設定します。</DialogDescription>
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
                        {errors.name && <p className="text-xs text-red-600">{errors.name}</p>}
                    </div>

                    {/* URL */}
                    <div className="space-y-2">
                        <Label htmlFor="schedule-url">Zoom 会議URL</Label>
                        <Input
                            id="schedule-url"
                            placeholder="https://zoom.us/j/123456789?pwd=..."
                            value={url}
                            onChange={(e) => setUrl(e.target.value)}
                        />
                        {errors.url ? (
                            <p className="text-xs text-red-600">{errors.url}</p>
                        ) : (
                            <p className="text-xs text-zinc-400">
                                パスコード付きの招待リンク（pwd=...）を貼り付けると入力なしで参加できます
                            </p>
                        )}
                    </div>

                    {/* スケジュール種別 */}
                    <div className="space-y-2">
                        <Label>スケジュール種別</Label>
                        <Select value={scheduleMode} onValueChange={(v) => setScheduleMode(v as 'once' | 'weekly')}>
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
                            <Label htmlFor="schedule-datetime">開始日時</Label>
                            <Input
                                id="schedule-datetime"
                                type="datetime-local"
                                value={dateTime}
                                onChange={(e) => setDateTime(e.target.value)}
                            />
                            {errors.datetime && <p className="text-xs text-red-600">{errors.datetime}</p>}
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
                                        {[0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55].map((m) => (
                                            <SelectItem key={m} value={String(m)}>
                                                {String(m).padStart(2, '0')}
                                            </SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            </div>
                        </div>
                    )}

                    {/* 録画時間 */}
                    <div className="space-y-3">
                        <div className="flex items-center justify-between">
                            <Label htmlFor="has-duration" className="cursor-pointer">
                                録画時間を指定
                            </Label>
                            <Switch id="has-duration" checked={hasDuration} onCheckedChange={setHasDuration} />
                        </div>
                        {hasDuration ? (
                            <div className="space-y-1">
                                <div className="flex items-center gap-2">
                                    <Input
                                        type="number"
                                        min={1}
                                        max={720}
                                        value={durationMinutes}
                                        onChange={(e) => setDurationMinutes(e.target.value)}
                                        className="w-24"
                                    />
                                    <span className="text-sm text-zinc-500">分後に自動停止（会議終了時も停止）</span>
                                </div>
                                {errors.duration && <p className="text-xs text-red-600">{errors.duration}</p>}
                            </div>
                        ) : (
                            <p className="text-xs text-zinc-400">会議が終了（退出）したら自動で録画を停止します</p>
                        )}
                    </div>
                </div>

                <DialogFooter>
                    <Button variant="outline" onClick={() => onOpenChange(false)} className="cursor-pointer">
                        キャンセル
                    </Button>
                    <Button
                        onClick={handleSave}
                        className="cursor-pointer bg-blue-600 hover:bg-blue-700 text-white"
                        disabled={saving || !name.trim() || !url.trim()}
                    >
                        {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                        {editSchedule ? '更新' : '追加'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
