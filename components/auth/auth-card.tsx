'use client';

import { Video } from 'lucide-react';

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

/** 認証画面の共通レイアウト */
export default function AuthCard({
    title,
    description,
    children,
    footer,
}: {
    title: string;
    description?: React.ReactNode;
    children: React.ReactNode;
    footer?: React.ReactNode;
}) {
    return (
        <div className="min-h-screen flex items-center justify-center bg-zinc-50 dark:bg-zinc-950 p-4">
            <Card className="w-full max-w-sm shadow-xl border-zinc-200/60 dark:border-zinc-800/60">
                <CardHeader className="space-y-4 pb-4 pt-8">
                    <div className="flex justify-center">
                        <div className="p-3 bg-blue-100 dark:bg-blue-900/30 rounded-full">
                            <Video className="w-6 h-6 text-blue-600 dark:text-blue-400" />
                        </div>
                    </div>
                    <div className="space-y-2 text-center">
                        <CardTitle className="text-2xl font-bold tracking-tight">{title}</CardTitle>
                        {description && (
                            <CardDescription className="text-zinc-500 dark:text-zinc-400 text-sm">
                                {description}
                            </CardDescription>
                        )}
                    </div>
                </CardHeader>
                <CardContent className="pb-6 space-y-4">
                    {children}
                    {footer && (
                        <div className="pt-2 text-center text-sm text-zinc-500 space-y-1">{footer}</div>
                    )}
                </CardContent>
            </Card>
        </div>
    );
}

/** フォーム項目のエラーメッセージ */
export function FieldError({ message }: { message?: string }) {
    if (!message) return null;
    return <p className="text-xs text-red-600">{message}</p>;
}
