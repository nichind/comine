declare global {
  namespace App {}

  const __DEV__: boolean;
  const __COMMIT_HASH__: string;
  const __GIT_BRANCH__: string;
  const __APP_VERSION__: string;
  const __BUILD_DATE__: string;

  interface Window {
    AndroidColors?: {
      getMaterialColors(): string;
      getWallpaperColors?: () => string;
    };
    i18n?: {
      t: (key: string) => string;
      locale: string;
    };
    __androidLog?: (level: string, source: string, message: string) => void;
  }
}

export {};
