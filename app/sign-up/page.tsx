/* 新規登録ページ（Clerk: メールアドレス + パスワード、メールの確認コードで認証） */

'use client';

import { useState } from 'react';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { useSignUp } from '@clerk/react/legacy';
import { Loader2 } from 'lucide-react';
import toast from 'react-hot-toast';

import AuthCard, { FieldError } from '@/components/auth/auth-card';
import AuthGuard from '@/components/auth/auth-guard';
import GoogleSsoButton from '@/components/auth/google-sso-button';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { clerkErrorMessage } from '@/lib/clerk-errors';
import { autoConnectGoogleDrive } from '@/lib/drive-connect';
import {
    signUpFormSchema,
    verificationCodeSchema,
    type SignUpFormValues,
    type VerificationCodeValues,
} from '@/lib/validation';

export default function SignUpPage() {
    return (
        <AuthGuard requireSignedOut>
            <SignUpForm />
        </AuthGuard>
    );
}

function SignUpForm() {
    const { isLoaded, signUp, setActive } = useSignUp();
    const router = useRouter();
    const [pendingVerification, setPendingVerification] = useState(false);

    const signUpForm = useForm<SignUpFormValues>({
        resolver: zodResolver(signUpFormSchema),
        defaultValues: { lastName: '', firstName: '', email: '', password: '', confirmPassword: '' },
    });
    const codeForm = useForm<VerificationCodeValues>({
        resolver: zodResolver(verificationCodeSchema),
        defaultValues: { code: '' },
    });

    const onSubmitSignUp = async (values: SignUpFormValues) => {
        if (!isLoaded) return;
        try {
            await signUp.create({
                emailAddress: values.email,
                password: values.password,
                firstName: values.firstName,
                lastName: values.lastName,
            });
            await signUp.prepareEmailAddressVerification({ strategy: 'email_code' });
            setPendingVerification(true);
            toast.success('確認コードをメールで送信しました');
        } catch (err) {
            toast.error(clerkErrorMessage(err, '登録に失敗しました'));
        }
    };

    const onSubmitCode = async (values: VerificationCodeValues) => {
        if (!isLoaded) return;
        try {
            const result = await signUp.attemptEmailAddressVerification({ code: values.code });
            if (result.status === 'complete') {
                await setActive({ session: result.createdSessionId });
                toast.success('アカウントを作成しました');
                void autoConnectGoogleDrive();
                router.replace('/dashboard');
            } else {
                toast.error('登録を完了できませんでした。入力内容を確認してください');
            }
        } catch (err) {
            toast.error(clerkErrorMessage(err, '認証に失敗しました'));
        }
    };

    const resendCode = async () => {
        if (!isLoaded) return;
        try {
            await signUp.prepareEmailAddressVerification({ strategy: 'email_code' });
            toast.success('確認コードを再送信しました');
        } catch (err) {
            toast.error(clerkErrorMessage(err));
        }
    };

    if (pendingVerification) {
        const { register, handleSubmit, formState } = codeForm;
        return (
            <AuthCard
                title="メールアドレスの確認"
                description="メールに届いた確認コードを入力してください"
                footer={
                    <button
                        type="button"
                        className="text-blue-600 hover:underline cursor-pointer"
                        onClick={resendCode}
                    >
                        コードを再送信
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
                        登録を完了
                    </Button>
                </form>
            </AuthCard>
        );
    }

    const { register, handleSubmit, formState } = signUpForm;
    return (
        <AuthCard
            title="新規登録"
            description="Auto Meeting Capture のアカウントを作成します"
            footer={
                <p>
                    既にアカウントをお持ちの方は{' '}
                    <Link href="/login" className="text-blue-600 hover:underline">
                        ログイン
                    </Link>
                </p>
            }
        >
            <GoogleSsoButton label="Google で登録" />
            <form onSubmit={handleSubmit(onSubmitSignUp)} className="space-y-4">
                <div className="grid grid-cols-2 gap-3">
                    <div className="space-y-2">
                        <Label htmlFor="lastName">姓</Label>
                        <Input id="lastName" autoComplete="family-name" {...register('lastName')} />
                        <FieldError message={formState.errors.lastName?.message} />
                    </div>
                    <div className="space-y-2">
                        <Label htmlFor="firstName">名</Label>
                        <Input id="firstName" autoComplete="given-name" {...register('firstName')} />
                        <FieldError message={formState.errors.firstName?.message} />
                    </div>
                </div>
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
                        autoComplete="new-password"
                        {...register('password')}
                    />
                    <FieldError message={formState.errors.password?.message} />
                </div>
                <div className="space-y-2">
                    <Label htmlFor="confirmPassword">パスワード（確認）</Label>
                    <Input
                        id="confirmPassword"
                        type="password"
                        autoComplete="new-password"
                        {...register('confirmPassword')}
                    />
                    <FieldError message={formState.errors.confirmPassword?.message} />
                </div>
                {/* Clerk の Bot 対策（CAPTCHA）ウィジェットの表示位置 */}
                <div id="clerk-captcha" />
                <Button
                    type="submit"
                    className="w-full h-11 cursor-pointer bg-blue-600 hover:bg-blue-700 text-white"
                    disabled={!isLoaded || formState.isSubmitting}
                >
                    {formState.isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                    登録する
                </Button>
            </form>
        </AuthCard>
    );
}
