import toast from 'react-hot-toast';

import { connectGoogleDrive, errorMessage, getDriveAuthStatus, isTauri } from '@/lib/tauri';

const TOAST_ID = 'drive-auto-connect';

/**
 * ログイン直後に Google Drive の連携を自動で開始する（未連携のときだけ）。
 * Drive の同意はシステムブラウザで行うため、完了を待たずに呼び出す（画面遷移をブロックしない）。
 */
export async function autoConnectGoogleDrive(): Promise<void> {
    if (!isTauri()) return;
    try {
        if ((await getDriveAuthStatus()).connected) return;
        toast.loading('ブラウザで Google Drive の連携を許可してください', { id: TOAST_ID });
        await connectGoogleDrive();
        toast.success('Google Drive と連携しました', { id: TOAST_ID });
    } catch (err) {
        toast.error(`Google Drive と連携できませんでした（設定から再試行できます）: ${errorMessage(err)}`, {
            id: TOAST_ID,
        });
    }
}
