/* Clerk のエラーを日本語メッセージに変換 */

import { isClerkAPIResponseError } from '@clerk/react/errors';

const MESSAGES: Record<string, string> = {
    form_identifier_not_found: 'このメールアドレスのアカウントが見つかりません',
    form_password_incorrect: 'パスワードが正しくありません',
    form_identifier_exists: 'このメールアドレスは既に登録されています',
    form_password_pwned: 'このパスワードは漏えいが確認されているため使用できません。別のパスワードを設定してください',
    form_password_length_too_short: 'パスワードが短すぎます',
    form_password_validation_failed: 'パスワードが正しくありません',
    form_code_incorrect: '認証コードが正しくありません',
    verification_expired: '認証コードの有効期限が切れました。再送信してください',
    verification_failed: '認証に失敗しました。もう一度お試しください',
    form_param_format_invalid: '入力形式が正しくありません',
    too_many_requests: '試行回数が多すぎます。しばらく待ってから再度お試しください',
    session_exists: '既にログインしています',
};

export function clerkErrorMessage(err: unknown, fallback = 'エラーが発生しました。もう一度お試しください'): string {
    if (isClerkAPIResponseError(err)) {
        const first = err.errors[0];
        if (first) {
            return MESSAGES[first.code] ?? first.longMessage ?? first.message ?? fallback;
        }
    }
    if (err instanceof Error && err.message) return err.message;
    return fallback;
}
