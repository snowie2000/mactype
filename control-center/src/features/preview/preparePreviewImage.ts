import { previewImageUrl } from "../../app/tauri";

// A helper response names a file; it does not mean the browser has decoded it.
export async function preparePreviewImage(path: string): Promise<void> {
  const image = new Image();
  image.src = previewImageUrl(path);
  await image.decode();
}
