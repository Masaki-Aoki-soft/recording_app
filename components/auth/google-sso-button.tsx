'use client';

import { Loader2 } from 'lucide-react';
import { FcGoogle } from 'react-icons/fc';

import { Button } from '@/components/ui/button';
import { useGoogleSso } from '@/hooks/use-google-sso';

/** 「Google でログイン」ボタン + 区切り線 */
export default function GoogleSsoButton({ label = 'Google でログイン' }: { label?: string }) {
    const { start, cancel, pending, ready } = useGoogleSso();

    return (
        <div className="space-y-4">
            <Button
                type="button"
                variant="outline"
                className="cursor-pointer w-full h-11 text-base font-medium transition-all hover:bg-zinc-100 dark:hover:bg-zinc-800"
                onClick={start}
                disabled={!ready || pending}
            >
                {pending ? (
                    <Loader2 className="mr-2 h-5 w-5 animate-spin text-zinc-500" />
                ) : (
                    <FcGoogle className="mr-2 h-5 w-5" />
                )}
                {pending ? 'ブラウザで認証しています...' : label}
            </Button>
            {pending && (
                <p className="text-center text-xs text-zinc-500">
                    ブラウザで Google アカウントを選択してください。{' '}
                    <button type="button" className="text-blue-600 hover:underline cursor-pointer" onClick={cancel}>
                        キャンセル
                    </button>
                </p>
            )}
            <div className="flex items-center gap-3 text-xs text-zinc-400">
                <div className="h-px flex-1 bg-zinc-200 dark:bg-zinc-800" />
                または
                <div className="h-px flex-1 bg-zinc-200 dark:bg-zinc-800" />
            </div>
        </div>
    );
}
