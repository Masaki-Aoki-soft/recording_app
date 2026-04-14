/* ログインページ */

'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Loader2, Video } from 'lucide-react';
import { FcGoogle } from 'react-icons/fc';
import toast from 'react-hot-toast';
export default function LoginPage() {
    const [isLoading, setIsLoading] = useState(false);
    const router = useRouter();

    const handleGoogleLogin = async () => {
        setIsLoading(true);

        await toast.promise(
            new Promise<string>(async (resolve, reject) => {
                try {
                    await invoke('start_google_auth');

                    resolve('ログインに成功しました');
                    router.push('/dashboard');
                } catch (error: any) {
                    reject(error);
                    // Rust側から返されたエラーメッセージがあれば表示
                    const errorMessage =
                        typeof error === 'string'
                            ? error
                            : 'ログインに失敗しました。再度お試しください。';
                    reject(errorMessage);
                } finally {
                    setIsLoading(false);
                }
            }),
            {
                loading: 'ログイン中...',
                success: (message: string) => message,
                error: (message: string) => message,
            }
        );
    };

    return (
        <div className="min-h-screen flex items-center justify-center bg-zinc-50 dark:bg-zinc-950 p-4">
            <Card className="w-full max-w-sm shadow-xl border-zinc-200/60 dark:border-zinc-800/60">
                <CardHeader className="space-y-4 pb-6 pt-8">
                    <div className="flex justify-center">
                        <div className="p-3 bg-blue-100 dark:bg-blue-900/30 rounded-full">
                            <Video className="w-6 h-6 text-blue-600 dark:text-blue-400" />
                        </div>
                    </div>
                    <div className="space-y-2 text-center">
                        <CardTitle className="text-2xl font-bold tracking-tight">
                            ログイン
                        </CardTitle>
                        <CardDescription className="text-zinc-500 dark:text-zinc-400 text-sm">
                            Google Driveへの保存を有効にするために
                            <br />
                            アカウントを連携してください
                        </CardDescription>
                    </div>
                </CardHeader>

                <CardContent className="pb-6">
                    <Button
                        variant="outline"
                        className="cursor-pointer w-full h-12 text-base font-medium transition-all hover:bg-zinc-100 dark:hover:bg-zinc-800"
                        onClick={handleGoogleLogin}
                        disabled={isLoading}
                    >
                        {isLoading ? (
                            <Loader2 className="mr-2 h-5 w-5 animate-spin text-zinc-500" />
                        ) : (
                            <FcGoogle className="mr-2 h-5 w-5" />
                        )}
                        {isLoading ? 'ブラウザで認証待ち...' : 'Googleでログイン'}
                    </Button>
                </CardContent>
            </Card>
        </div>
    );
}
