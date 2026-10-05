// Work-in-progress sections show only in builds with VITE_SHOW_WIP=true, set in an ignored .env.*.local file.
export const showWip = import.meta.env.VITE_SHOW_WIP === 'true';
