/** Bumped by the settings page after a save so the sidebar's connection line refreshes. */
export const useSettingsSaved = () => useState<number>('settings-saved', () => 0);
