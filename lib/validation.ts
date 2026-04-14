/* バリデーションスキーマ */

'use client';

import * as z from 'zod';

// ログインフォームのバリデーションスキーマ定義
export const loginFormSchema = z.object({
    email: z
        .string()
        .nonempty('パスワードを入力してください')
        .email({ message: '有効なメールアドレスを入力してください' }),
    password: z.string().min(6, { message: 'パスワードは6文字以上で入力してください' }),
});

export type LoginFormValues = z.infer<typeof loginFormSchema>;

// デバイス登録フォームのバリデーションスキーマ定義
export const deviceSchema = z.object({
    name: z.string().nonempty('デバイス名は必須です'),
    macAddress: z
        .string()
        .nonempty('MACアドレスは必須です')
        .regex(/^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/, '無効なMACアドレス形式です'),
    description: z.string().optional(),
});

export type DeviceFormValues = z.infer<typeof deviceSchema>;

// パスワード再設定フォーム用バリデーション
export const forgotPasswordSchema = z.object({
    email: z
        .string()
        .nonempty('メールアドレスを入力してください')
        .email('有効なメールアドレスを入力してください'),
});

export type ForgotPasswordValues = z.infer<typeof forgotPasswordSchema>;

// 認証後のパスワード再設定フォーム用バリデーション
export const resetPasswordSchema = z
    .object({
        code: z
            .string()
            .nonempty('認証コードを入力してください')
            .min(6, '認証コードは6文字以上で入力してください'),
        password: z
            .string()
            .nonempty('パスワードを入力してください')
            .min(8, 'パスワードは8文字以上で入力してください'),
        confirmPassword: z.string().min(1, 'パスワード（確認）を入力してください'),
    })
    .refine((data) => data.password === data.confirmPassword, {
        message: 'パスワードが一致しません',
        path: ['confirmPassword'],
    });

export type ResetPasswordValues = z.infer<typeof resetPasswordSchema>;

// 新規登録ページのバリデーション
export const signUpFormSchema = z
    .object({
        firstName: z.string().nonempty('名前を入力してください'),
        lastName: z.string().nonempty('姓を入力してください'),
        email: z.string().email('有効なメールアドレスを入力してください'),
        password: z.string().min(8, 'パスワードは8文字以上で入力してください'),
        confirmPassword: z.string(),
    })
    .refine((data) => data.password === data.confirmPassword, {
        message: 'パスワードが一致しません',
        path: ['confirmPassword'],
    });

export type SignUpFormValues = z.infer<typeof signUpFormSchema>;
