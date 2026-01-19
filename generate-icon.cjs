const fs = require('fs');
const path = require('path');
const sharp = require('sharp');

const svgPath = path.join(__dirname, 'src-tauri', 'icons', 'icon.svg');
const icoPath = path.join(__dirname, 'src-tauri', 'icons', 'icon.ico');
const pngPath = path.join(__dirname, 'src-tauri', 'icons', 'icon.png');
const png32Path = path.join(__dirname, 'src-tauri', 'icons', '32x32.png');
const png128Path = path.join(__dirname, 'src-tauri', 'icons', '128x128.png');
const png256Path = path.join(__dirname, 'src-tauri', 'icons', '128x128@2x.png');

async function generateIcons() {
  const svgBuffer = fs.readFileSync(svgPath);

  // Windows ICO should have these sizes for crisp display at all DPI levels
  // 16, 20, 24, 32, 40, 48, 64, 256 are the key sizes
  const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];

  console.log('Generating PNG images at multiple sizes...');

  // Generate high-quality PNGs at each size
  const pngBuffers = await Promise.all(
    sizes.map(async size => {
      // Render SVG at 4x resolution then downscale for better quality
      const renderSize = Math.min(size * 4, 1024);
      const buffer = await sharp(svgBuffer, { density: 300 })
        .resize(renderSize, renderSize, {
          fit: 'contain',
          background: { r: 0, g: 0, b: 0, alpha: 0 }
        })
        .resize(size, size, {
          fit: 'contain',
          kernel: 'lanczos3'
        })
        .png({ compressionLevel: 9 })
        .toBuffer();
      return { size, data: buffer };
    })
  );

  // Save individual PNGs for Tauri
  await sharp(svgBuffer, { density: 300 })
    .resize(256, 256, { kernel: 'lanczos3' })
    .png()
    .toFile(pngPath);
  console.log('Generated icon.png (256x256)');

  await sharp(svgBuffer, { density: 300 })
    .resize(32, 32, { kernel: 'lanczos3' })
    .png()
    .toFile(png32Path);
  console.log('Generated 32x32.png');

  await sharp(svgBuffer, { density: 300 })
    .resize(128, 128, { kernel: 'lanczos3' })
    .png()
    .toFile(png128Path);
  console.log('Generated 128x128.png');

  await sharp(svgBuffer, { density: 300 })
    .resize(256, 256, { kernel: 'lanczos3' })
    .png()
    .toFile(png256Path);
  console.log('Generated 128x128@2x.png');

  // Create ICO file with PNG images embedded
  // ICO format: Header (6 bytes) + Directory entries (16 bytes each) + Image data
  console.log('Creating ICO file...');

  const numImages = pngBuffers.length;

  // ICO Header
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);           // Reserved, must be 0
  header.writeUInt16LE(1, 2);           // Image type: 1 = ICO
  header.writeUInt16LE(numImages, 4);   // Number of images

  // Calculate offsets
  const headerSize = 6;
  const directorySize = 16 * numImages;
  let currentOffset = headerSize + directorySize;

  // Create directory entries
  const directories = [];
  for (const img of pngBuffers) {
    const dir = Buffer.alloc(16);
    dir.writeUInt8(img.size >= 256 ? 0 : img.size, 0);   // Width (0 = 256)
    dir.writeUInt8(img.size >= 256 ? 0 : img.size, 1);   // Height (0 = 256)
    dir.writeUInt8(0, 2);                                 // Color palette (0 = no palette)
    dir.writeUInt8(0, 3);                                 // Reserved
    dir.writeUInt16LE(1, 4);                              // Color planes
    dir.writeUInt16LE(32, 6);                             // Bits per pixel
    dir.writeUInt32LE(img.data.length, 8);                // Size of image data
    dir.writeUInt32LE(currentOffset, 12);                 // Offset to image data
    directories.push(dir);
    currentOffset += img.data.length;
  }

  // Combine all parts
  const ico = Buffer.concat([
    header,
    ...directories,
    ...pngBuffers.map(img => img.data)
  ]);

  fs.writeFileSync(icoPath, ico);
  console.log(`Generated icon.ico with ${numImages} sizes: ${sizes.join(', ')}px`);
  console.log(`ICO file size: ${(ico.length / 1024).toFixed(1)} KB`);
}

generateIcons().catch(err => {
  console.error('Error generating icons:', err);
  process.exit(1);
});
