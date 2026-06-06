export function getRozsaUserAgent(version: string): string {
	const runtime = process.versions.bun ? `bun/${process.versions.bun}` : `node/${process.version}`;
	return `rozsa/${version} (${process.platform}; ${runtime}; ${process.arch})`;
}
