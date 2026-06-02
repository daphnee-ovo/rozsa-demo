import { access, readFile, stat } from "node:fs/promises";
import { resolve } from "node:path";
import type { ImageContent } from "@earendil-works/pi-ai";
import { resolveReadPath } from "../../core/tools/path-utils.ts";
import { formatDimensionNote, resizeImage } from "../../utils/image-resize.ts";
import { detectSupportedImageMimeTypeFromFile } from "../../utils/mime.ts";

export interface NativeFileExpansion {
	text: string;
	images?: ImageContent[];
}

interface FileMention {
	path: string;
}

export async function expandNativeFileReferences(
	text: string,
	options: { cwd: string; autoResizeImages: boolean },
): Promise<NativeFileExpansion> {
	const mentions = findFileMentions(text);
	if (mentions.length === 0) return { text };

	let fileText = "";
	const images: ImageContent[] = [];
	const seen = new Set<string>();
	for (const mention of mentions) {
		const absolutePath = resolve(resolveReadPath(mention.path, options.cwd));
		if (seen.has(absolutePath)) continue;
		if (!(await fileExists(absolutePath))) continue;
		seen.add(absolutePath);

		const stats = await stat(absolutePath);
		if (stats.size === 0) continue;
		const mimeType = await detectSupportedImageMimeTypeFromFile(absolutePath);
		if (mimeType) {
			const content = await readFile(absolutePath);
			const image = options.autoResizeImages ? await resizeImage(content, mimeType) : undefined;
			if (options.autoResizeImages && !image) {
				fileText += `<file name="${absolutePath}">[Image omitted: could not be resized below the inline image size limit.]</file>\n`;
				continue;
			}
			const attachment = image
				? { type: "image" as const, mimeType: image.mimeType, data: image.data }
				: { type: "image" as const, mimeType, data: content.toString("base64") };
			images.push(attachment);
			const note = image ? formatDimensionNote(image) : "";
			fileText += `<file name="${absolutePath}">${note}</file>\n`;
		} else {
			const content = await readFile(absolutePath, "utf-8");
			fileText += `<file name="${absolutePath}">\n${content}\n</file>\n`;
		}
	}

	if (!fileText) return { text };
	return {
		text: `${fileText}\n${text}`,
		images: images.length > 0 ? images : undefined,
	};
}

function findFileMentions(text: string): FileMention[] {
	const mentions: FileMention[] = [];
	for (let i = 0; i < text.length; i++) {
		if (text[i] !== "@") continue;
		if (i > 0 && !/\s/.test(text[i - 1] ?? "")) continue;
		const quoted = text[i + 1] === '"';
		if (quoted) {
			const end = text.indexOf('"', i + 2);
			if (end === -1) continue;
			const path = text.slice(i + 2, end);
			if (path) mentions.push({ path });
			i = end;
			continue;
		}
		let end = i + 1;
		while (end < text.length && !/\s/.test(text[end] ?? "")) end++;
		const path = text.slice(i + 1, end);
		if (path) mentions.push({ path });
		i = end;
	}
	return mentions;
}

async function fileExists(path: string): Promise<boolean> {
	try {
		await access(path);
		return true;
	} catch {
		return false;
	}
}
