param(
    [Parameter(Mandatory = $true)]
    [string] $Source
)

Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$sourcePath = (Resolve-Path $Source).Path
$logoPath = Join-Path $root "assets\cc22-logo.png"
$iconPath = Join-Path $root "assets\cc22.ico"
$rgbaPath = Join-Path $root "src\ui\logo_rgba.bin"

Copy-Item -LiteralPath $sourcePath -Destination $logoPath -Force

function Get-LogoBounds([System.Drawing.Bitmap] $image) {
    $left = $image.Width
    $top = $image.Height
    $right = -1
    $bottom = -1

    for ($y = 0; $y -lt $image.Height; $y++) {
        for ($x = 0; $x -lt $image.Width; $x++) {
            if ($image.GetPixel($x, $y).A -gt 4) {
                if ($x -lt $left) { $left = $x }
                if ($x -gt $right) { $right = $x }
                if ($y -lt $top) { $top = $y }
                if ($y -gt $bottom) { $bottom = $y }
            }
        }
    }

    if ($right -lt $left -or $bottom -lt $top) {
        throw "The source image is fully transparent."
    }

    $width = $right - $left + 1
    $height = $bottom - $top + 1
    $side = [Math]::Max($width, $height)
    $padding = [Math]::Ceiling($side * 0.06)
    $side += 2 * $padding
    $centerX = ($left + $right) / 2.0
    $centerY = ($top + $bottom) / 2.0
    return [System.Drawing.RectangleF]::new(
        [single]($centerX - $side / 2.0),
        [single]($centerY - $side / 2.0),
        [single]$side,
        [single]$side
    )
}

function New-LogoBitmap(
    [System.Drawing.Bitmap] $source,
    [System.Drawing.RectangleF] $sourceBounds,
    [int] $size
) {
    $result = [System.Drawing.Bitmap]::new(
        $size,
        $size,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
    )
    $result.SetResolution(96, 96)
    $graphics = [System.Drawing.Graphics]::FromImage($result)
    try {
        $graphics.Clear([System.Drawing.Color]::Transparent)
        $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
        $graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
        $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
        $destination = [System.Drawing.RectangleF]::new(0, 0, $size, $size)
        $graphics.DrawImage($source, $destination, $sourceBounds, [System.Drawing.GraphicsUnit]::Pixel)
    }
    finally {
        $graphics.Dispose()
    }
    return $result
}

$sourceImage = [System.Drawing.Bitmap]::new($logoPath)
try {
    $bounds = Get-LogoBounds $sourceImage

    $uiImage = New-LogoBitmap $sourceImage $bounds 80
    try {
        $stream = [System.IO.File]::Open($rgbaPath, [System.IO.FileMode]::Create)
        $writer = [System.IO.BinaryWriter]::new($stream)
        try {
            for ($y = 0; $y -lt $uiImage.Height; $y++) {
                for ($x = 0; $x -lt $uiImage.Width; $x++) {
                    $pixel = $uiImage.GetPixel($x, $y)
                    $writer.Write([byte]$pixel.R)
                    $writer.Write([byte]$pixel.G)
                    $writer.Write([byte]$pixel.B)
                    $writer.Write([byte]$pixel.A)
                }
            }
        }
        finally {
            $writer.Dispose()
        }
    }
    finally {
        $uiImage.Dispose()
    }

    $sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
    $images = @()
    foreach ($size in $sizes) {
        $bitmap = New-LogoBitmap $sourceImage $bounds $size
        try {
            $memory = [System.IO.MemoryStream]::new()
            $bitmap.Save($memory, [System.Drawing.Imaging.ImageFormat]::Png)
            $images += ,$memory.ToArray()
            $memory.Dispose()
        }
        finally {
            $bitmap.Dispose()
        }
    }

    $iconStream = [System.IO.File]::Open($iconPath, [System.IO.FileMode]::Create)
    $iconWriter = [System.IO.BinaryWriter]::new($iconStream)
    try {
        $iconWriter.Write([uint16]0)
        $iconWriter.Write([uint16]1)
        $iconWriter.Write([uint16]$sizes.Count)
        $offset = 6 + (16 * $sizes.Count)

        for ($index = 0; $index -lt $sizes.Count; $index++) {
            $size = $sizes[$index]
            $iconWriter.Write([byte]($(if ($size -eq 256) { 0 } else { $size })))
            $iconWriter.Write([byte]($(if ($size -eq 256) { 0 } else { $size })))
            $iconWriter.Write([byte]0)
            $iconWriter.Write([byte]0)
            $iconWriter.Write([uint16]1)
            $iconWriter.Write([uint16]32)
            $iconWriter.Write([uint32]$images[$index].Length)
            $iconWriter.Write([uint32]$offset)
            $offset += $images[$index].Length
        }

        foreach ($imageBytes in $images) {
            $iconWriter.Write($imageBytes)
        }
    }
    finally {
        $iconWriter.Dispose()
    }
}
finally {
    $sourceImage.Dispose()
}
