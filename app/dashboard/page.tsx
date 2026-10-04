'use client';

import { useEffect, useRef, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useClerk, useUser } from '@clerk/react';
import { Calendar, LogOut, Settings, User, Video } from 'lucide-react';
import toast from 'react-hot-toast';

import AuthGuard from '@/components/auth/auth-guard';
import RecordTab from '@/components/dashboard/record-tab';
import ScheduleTab from '@/components/dashboard/schedule-tab';
import SettingsTab from '@/components/dashboard/settings-tab';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { useElapsedSeconds, useSessionStatus } from '@/hooks/use-session-status';
import {
    disconnectGoogleDrive,
    formatElapsedTime,
    isTauri,
    onUploadProgress,
    setSignedIn,
} from '@/lib/tauri';

type Tab = 'record' | 'schedule' | 'settings';

const TABS: { id: Tab; label: string; icon: typeof Video }[] = [
    { id: 'record', label: '録画', icon: Video },
    { id: 'schedule', label: 'スケジュール', icon: Calendar },
    { id: 'settings', label: '設定・アカウント', icon: Settings },
];

export default function DashboardPage() {
    return (
        <AuthGuard>
            <Dashboard />
        </AuthGuard>
    );
}

function Dashboard() {
    const [activeTab, setActiveTab] = useState<Tab>('record');
    const [isSidebarOpen, setIsSidebarOpen] = useState(false);
    const menuRef = useRef<HTMLElement>(null);
    const buttonRef = useRef<HTMLDivElement>(null);

    const { user } = useUser();
    const { signOut } = useClerk();
    const session = useSessionStatus();
    const elapsed = useElapsedSeconds(session.state === 'recording' ? session.started_at : null);

    // アップロード結果の通知
    useEffect(() => {
        if (!isTauri()) return;
        let unlisten: (() => void) | undefined;
        onUploadProgress((payload) => {
            if (payload.status === 'completed') {
                toast.success(`${payload.file_name} を Google Drive にアップロードしました`);
            } else if (payload.status === 'error') {
                toast.error(`${payload.file_name} のアップロードに失敗しました`);
            }
        }).then((fn) => (unlisten = fn));
        return () => unlisten?.();
    }, []);

    // メニュー外をクリックしたときに閉じる
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
        return () => document.removeEventListener('mousedown', handleClickOutside);
    }, [isSidebarOpen]);

    const handleLogout = async () => {
        if (session.state !== 'idle' && session.state !== 'error') {
            toast.error('録画中はログアウトできません。録画を停止してから再度お試しください');
            return;
        }
        try {
            if (isTauri()) {
                // 次にログインする人のアップロード先にならないよう、先に Drive の連携を解除する
                await disconnectGoogleDrive();
                await setSignedIn(false);
            }
            await signOut({ redirectUrl: '/login' });
            toast.success('ログアウトしました');
        } catch (err) {
            console.error(err);
            toast.error('ログアウトに失敗しました');
        }
    };

    const displayName = user?.fullName || user?.primaryEmailAddress?.emailAddress || 'ユーザー';

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

                <h1 className="ml-4 text-xl font-bold flex items-center gap-2 text-zinc-900 dark:text-zinc-50">
                    <Video className="text-blue-600 h-6 w-6" /> Auto Meeting Capture
                </h1>

                {/* 録画中インジケーター */}
                {session.state === 'recording' && (
                    <div className="ml-auto flex items-center gap-2">
                        <span className="relative flex h-3 w-3">
                            <span className="animate-ping absolute inline-flex h-3 w-3 rounded-full bg-red-400 opacity-75"></span>
                            <span className="relative inline-flex rounded-full h-3 w-3 bg-red-500"></span>
                        </span>
                        <span className="text-sm font-mono text-red-600 dark:text-red-400">
                            REC {formatElapsedTime(elapsed)}
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
                                {TABS.map(({ id, label, icon: Icon }) => (
                                    <Button
                                        key={id}
                                        variant={activeTab === id ? 'secondary' : 'ghost'}
                                        className="w-full justify-start text-base cursor-pointer"
                                        onClick={() => {
                                            setActiveTab(id);
                                            setIsSidebarOpen(false);
                                        }}
                                    >
                                        <Icon className="mr-3 h-5 w-5" /> {label}
                                    </Button>
                                ))}
                            </nav>

                            <div className="p-4 border-t border-zinc-200 dark:border-zinc-800 flex flex-col gap-3">
                                <div className="flex items-center px-2">
                                    <Avatar className="h-10 w-10 shrink-0">
                                        <AvatarImage src={user?.imageUrl} />
                                        <AvatarFallback className="bg-blue-100 text-blue-700">
                                            <User className="h-5 w-5" />
                                        </AvatarFallback>
                                    </Avatar>
                                    <div className="flex flex-col ml-3 min-w-0">
                                        <span className="text-sm font-medium text-zinc-900 dark:text-zinc-100 truncate">
                                            {displayName}
                                        </span>
                                        <span className="text-[11px] text-zinc-500 truncate">
                                            {user?.primaryEmailAddress?.emailAddress}
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
                    {!isTauri() && (
                        <div className="rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">
                            ブラウザでプレビュー中のため、録画・スケジュール機能は動作しません（Tauri アプリとして起動してください）。
                        </div>
                    )}
                    {activeTab === 'record' && <RecordTab session={session} elapsed={elapsed} />}
                    {activeTab === 'schedule' && <ScheduleTab session={session} />}
                    {activeTab === 'settings' && <SettingsTab />}
                </div>
            </main>
        </div>
    );
}
