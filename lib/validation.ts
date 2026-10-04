/* バリデーションスキーマ */

import * as z from 'zod';

// ログインフォームのバリデーションスキーマ定義
export const loginFormSchema = z.object({
    email: z
        .string()
        .nonempty('メールアドレスを入力してください')
        .email({ message: '有効なメールアドレスを入力してください' }),
    password: z.string().min(6, { message: 'パスワードは6文字以上で入力してください' }),
});

export type LoginFormValues = z.infer<typeof loginFormSchema>;

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

// メールで届く確認コード
export const verificationCodeSchema = z.object({
    code: z
        .string()
        .trim()
        .nonempty('確認コードを入力してください')
        .min(6, '確認コードは6桁です'),
});

export type VerificationCodeValues = z.infer<typeof verificationCodeSchema>;

// Zoom 会議 URL（Rust 側の zoom::parse_meeting_url と同じ判定）
const ZOOM_HTTP_URL = /^https?:\/\/([a-z0-9-]+\.)*(zoom\.us|zoomgov\.com)(:\d+)?\/(j|w|s|wc\/join|wc)\/\d{9,}/i;
const ZOOM_PROTOCOL_URL = /^(zoommtg|zoomus):\/\/.*[?&]confno=\d{9,}/i;

export function isZoomMeetingUrl(url: string): boolean {
    const value = url.trim();
    return ZOOM_HTTP_URL.test(value) || ZOOM_PROTOCOL_URL.test(value);
}

// スケジュール登録フォーム
export const scheduleFormSchema = z.object({
    name: z.string().trim().nonempty('会議名を入力してください'),
    url: z
        .string()
        .trim()
        .nonempty('会議URLを入力してください')
        .refine(isZoomMeetingUrl, 'Zoom の会議URL（https://zoom.us/j/... など）を入力してください'),
});
