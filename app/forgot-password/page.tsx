/* パスワード再設定ページ（メールの確認コード → 新しいパスワード） */

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
    forgotPasswordSchema,
    resetPasswordSchema,
    type ForgotPasswordValues,
    type ResetPasswordValues,
} from '@/lib/validation';

export default function ForgotPasswordPage() {
    return (
        <AuthGuard requireSignedOut>
            <ForgotPasswordForm />
        </AuthGuard>
    );
}

function ForgotPasswordForm() {
    const { isLoaded, signIn, setActive } = useSignIn();
    const router = useRouter();
    const [codeSent, setCodeSent] = useState(false);

    const emailForm = useForm<ForgotPasswordValues>({
        resolver: zodResolver(forgotPasswordSchema),
        defaultValues: { email: '' },
    });
    const resetForm = useForm<ResetPasswordValues>({
        resolver: zodResolver(resetPasswordSchema),
        defaultValues: { code: '', password: '', confirmPassword: '' },
    });

    const onSubmitEmail = async (values: ForgotPasswordValues) => {
        if (!isLoaded) return;
        try {
            await signIn.create({
                strategy: 'reset_password_email_code',
                identifier: values.email,
            });
            setCodeSent(true);
            toast.success('確認コードをメールで送信しました');
        } catch (err) {
            toast.error(clerkErrorMessage(err, 'コードを送信できませんでした'));
        }
    };

    const onSubmitReset = async (values: ResetPasswordValues) => {
        if (!isLoaded) return;
        try {
            const result = await signIn.attemptFirstFactor({
                strategy: 'reset_password_email_code',
                code: values.code,
                password: values.password,
            });
            if (result.status === 'complete') {
                await setActive({ session: result.createdSessionId });
                toast.success('パスワードを再設定しました');
                router.replace('/dashboard');
            } else {
                toast.error('再設定を完了できませんでした。ログイン画面からお試しください');
                router.replace('/login');
            }
        } catch (err) {
            toast.error(clerkErrorMessage(err, '再設定に失敗しました'));
        }
    };

    const footer = (
        <p>
            <Link href="/login" className="text-blue-600 hover:underline">
                ログイン画面に戻る
            </Link>
        </p>
    );

    if (codeSent) {
        const { register, handleSubmit, formState } = resetForm;
        return (
            <AuthCard
                title="パスワードの再設定"
                description="メールに届いた確認コードと新しいパスワードを入力してください"
                footer={footer}
            >
                <form onSubmit={handleSubmit(onSubmitReset)} className="space-y-4">
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
                    <div className="space-y-2">
                        <Label htmlFor="password">新しいパスワード</Label>
                        <Input
                            id="password"
                            type="password"
                            autoComplete="new-password"
                            {...register('password')}
                        />
                        <FieldError message={formState.errors.password?.message} />
                    </div>
                    <div className="space-y-2">
                        <Label htmlFor="confirmPassword">新しいパスワード（確認）</Label>
                        <Input
                            id="confirmPassword"
                            type="password"
                            autoComplete="new-password"
                            {...register('confirmPassword')}
                        />
                        <FieldError message={formState.errors.confirmPassword?.message} />
                    </div>
                    <Button type="submit" className="w-full h-11 cursor-pointer" disabled={formState.isSubmitting}>
                        {formState.isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                        パスワードを再設定
                    </Button>
                </form>
            </AuthCard>
        );
    }

    const { register, handleSubmit, formState } = emailForm;
    return (
        <AuthCard
            title="パスワードをお忘れの方"
            description="登録したメールアドレスに確認コードを送信します"
            footer={footer}
        >
            <form onSubmit={handleSubmit(onSubmitEmail)} className="space-y-4">
                <div className="space-y-2">
                    <Label htmlFor="email">メールアドレス</Label>
                    <Input id="email" type="email" autoComplete="email" {...register('email')} />
                    <FieldError message={formState.errors.email?.message} />
                </div>
                <Button
                    type="submit"
                    className="w-full h-11 cursor-pointer bg-blue-600 hover:bg-blue-700 text-white"
                    disabled={!isLoaded || formState.isSubmitting}
                >
                    {formState.isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                    確認コードを送信
                </Button>
            </form>
        </AuthCard>
    );
}
