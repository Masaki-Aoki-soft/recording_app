/* ログインページ（Clerk: メールアドレス + パスワード） */

'use client';

import { useState } from 'react';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { useSignIn } from '@clerk/react/legacy';
import { Loader2 } from 'lucide-react';
import toast from 'react-hot-toast';

import AuthCard, { FieldError } from '@/components/auth/auth-card';
import AuthGuard from '@/components/auth/auth-guard';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { clerkErrorMessage } from '@/lib/clerk-errors';
import {
    loginFormSchema,
    verificationCodeSchema,
    type LoginFormValues,
    type VerificationCodeValues,
} from '@/lib/validation';

export default function LoginPage() {
    return (
        <AuthGuard requireSignedOut>
            <LoginForm />
        </AuthGuard>
    );
}

function LoginForm() {
    const { isLoaded, signIn, setActive } = useSignIn();
    const router = useRouter();
    // 新しい端末からのログインなどで追加のメール認証が必要な場合
    const [needsCode, setNeedsCode] = useState(false);

    const credentialsForm = useForm<LoginFormValues>({
        resolver: zodResolver(loginFormSchema),
        defaultValues: { email: '', password: '' },
    });
    const codeForm = useForm<VerificationCodeValues>({
        resolver: zodResolver(verificationCodeSchema),
        defaultValues: { code: '' },
    });

    const finish = async (sessionId: string | null) => {
        if (!isLoaded || !sessionId) return;
        await setActive({ session: sessionId });
        toast.success('ログインしました');
        router.replace('/dashboard');
    };

    const onSubmitCredentials = async (values: LoginFormValues) => {
        if (!isLoaded) return;
        try {
            const result = await signIn.create({
                identifier: values.email,
                password: values.password,
            });

            if (result.status === 'complete') {
                await finish(result.createdSessionId);
                return;
            }

            if (result.status === 'needs_second_factor' || result.status === 'needs_client_trust') {
                const emailFactor = result.supportedSecondFactors?.find(
                    (f) => f.strategy === 'email_code',
                );
                if (emailFactor && 'emailAddressId' in emailFactor) {
                    await signIn.prepareSecondFactor({
                        strategy: 'email_code',
                        emailAddressId: emailFactor.emailAddressId,
                    });
                    setNeedsCode(true);
                    toast.success('確認コードをメールで送信しました');
                    return;
                }
            }

            toast.error('このアカウントのログイン方法には対応していません');
        } catch (err) {
            toast.error(clerkErrorMessage(err, 'ログインに失敗しました'));
        }
    };

    const onSubmitCode = async (values: VerificationCodeValues) => {
        if (!isLoaded) return;
        try {
            const result = await signIn.attemptSecondFactor({
                strategy: 'email_code',
                code: values.code,
            });
            if (result.status === 'complete') {
                await finish(result.createdSessionId);
            } else {
                toast.error('認証を完了できませんでした');
            }
        } catch (err) {
            toast.error(clerkErrorMessage(err, '認証に失敗しました'));
        }
    };

    if (needsCode) {
        const { register, handleSubmit, formState } = codeForm;
        return (
            <AuthCard
                title="確認コードの入力"
                description={
                    <>
                        新しい端末からのログインです。
                        <br />
                        メールに届いた確認コードを入力してください。
                    </>
                }
                footer={
                    <button
                        type="button"
                        className="text-blue-600 hover:underline cursor-pointer"
                        onClick={() => setNeedsCode(false)}
                    >
                        ログイン画面に戻る
                    </button>
                }
            >
                <form onSubmit={handleSubmit(onSubmitCode)} className="space-y-4">
                    <div className="space-y-2">
                        <Label htmlFor="code">確認コード</Label>
                        <Input
                            id="code"
                            inputMode="numeric"
                            autoComplete="one-time-code"
                            {...register('code')}
                        />
                        <FieldError message={formState.errors.code?.message} />
                    </div>
                    <Button type="submit" className="w-full h-11 cursor-pointer" disabled={formState.isSubmitting}>
                        {formState.isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                        確認
                    </Button>
                </form>
            </AuthCard>
        );
    }

    const { register, handleSubmit, formState } = credentialsForm;
    return (
        <AuthCard
            title="ログイン"
            description="MeetingRec のアカウントでログインしてください"
            footer={
                <>
                    <p>
                        <Link href="/forgot-password" className="text-blue-600 hover:underline">
                            パスワードをお忘れの方
                        </Link>
                    </p>
                    <p>
                        アカウントをお持ちでない方は{' '}
                        <Link href="/sign-up" className="text-blue-600 hover:underline">
                            新規登録
                        </Link>
                    </p>
                </>
            }
        >
            <form onSubmit={handleSubmit(onSubmitCredentials)} className="space-y-4">
                <div className="space-y-2">
                    <Label htmlFor="email">メールアドレス</Label>
                    <Input id="email" type="email" autoComplete="email" {...register('email')} />
                    <FieldError message={formState.errors.email?.message} />
                </div>
                <div className="space-y-2">
                    <Label htmlFor="password">パスワード</Label>
                    <Input
                        id="password"
                        type="password"
                        autoComplete="current-password"
                        {...register('password')}
                    />
                    <FieldError message={formState.errors.password?.message} />
                </div>
                <Button
                    type="submit"
                    className="w-full h-11 cursor-pointer bg-blue-600 hover:bg-blue-700 text-white"
                    disabled={!isLoaded || formState.isSubmitting}
                >
                    {formState.isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                    ログイン
                </Button>
            </form>
        </AuthCard>
    );
}
