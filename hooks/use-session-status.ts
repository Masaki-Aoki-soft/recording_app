'use client';

import { useEffect, useState } from 'react';

import { getSessionStatus, isTauri, onSessionStatus, type SessionStatus } from '@/lib/tauri';

const IDLE: SessionStatus = {
    state: 'idle',
    schedule_name: null,
    started_at: null,
    output_path: null,
    message: null,
};

/** Rust 側の録画セッション状態を購読する */
export function useSessionStatus(): SessionStatus {
    const [status, setStatus] = useState<SessionStatus>(IDLE);

    useEffect(() => {
        if (!isTauri()) return;
        let unlisten: (() => void) | undefined;
        let disposed = false;

        getSessionStatus()
            .then((s) => !disposed && setStatus(s))
            .catch((err) => console.error('Failed to get session status:', err));
        onSessionStatus(setStatus).then((fn) => {
            if (disposed) fn();
            else unlisten = fn;
        });

        return () => {
            disposed = true;
            unlisten?.();
        };
    }, []);

    return status;
}

/** 録画開始からの経過秒数（1 秒ごとに更新） */
export function useElapsedSeconds(startedAt: string | null): number {
    const [now, setNow] = useState(() => Date.now());

    useEffect(() => {
        if (!startedAt) return;
        const timer = setInterval(() => setNow(Date.now()), 1000);
        return () => clearInterval(timer);
    }, [startedAt]);

    if (!startedAt) return 0;
    return Math.max(0, Math.floor((now - new Date(startedAt).getTime()) / 1000));
}
