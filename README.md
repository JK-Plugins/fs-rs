# fs-rs

[English](./README.md) | [日本語](./README-ja.md)

## About

Those plugins are Rust port of the [F's Plugin](https://github.com/bryful/F-s-PluginsProjects).
By re-implementing the functionality of F's Plugin in Rust, we aim to add macOS support and achieve higher performance.

**Download: https://jk-plugins.github.io/JK-Plugins/**

## Plug-ins

| Plug-in | Version | Description |
|---|---|---|
| [JK Alpha Fix Fs](alphafix/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/alphafix-fs-v0.0.1) | Composites the input over a background color to make it opaque |
| [JK Alpha Hyperbolic Fs](alphahyperbolic/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/alphahyperbolic-fs-v0.0.1) | Applies a hyperbolic curve to the alpha channel |
| [JK Alpha Threshold Fs](alphathreshold/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/alphathreshold-fs-v0.0.1) | Thresholds the alpha channel to 0 or full |
| [JK Animated Noise Fs](animatednoise/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/animatednoise-fs-v0.0.1) | Overlays two layers of analog-style video noise |
| [JK CC Plus Fs](ccplus/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/ccplus-fs-v0.0.1) | Colorizes by luminance through a two-stop gradient |
| [JK Cell Grad Fs](cellgrad/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/cellgrad-fs-v0.0.1) | Fills the bounding box of a selected color with a gradient |
| [JK Cell Line Eraser Fs](celllineeraser/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/celllineeraser-fs-v0.0.1) | Erases the line art from cel-style artwork |
| [JK Channel Blur Fs](channelblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/channelblur-fs-v0.0.1) | Passes the input through (the original was left unimplemented) |
| [JK Channel Noise Fs](channelnoise/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/channelnoise-fs-v0.0.1) | Adds noise of a separate amount to each of R, G and B |
| [JK Channel Shift Fs](channelshift/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/channelshift-fs-v0.0.1) | Samples each channel from a different angle and distance |
| [JK Chromatic Aberration Fs](chromaticaberration/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/chromaticaberration-fs-v0.0.1) | Creates chromatic aberration by scaling R, G and B differently |
| [JK Color Change Eng Fs](colorchangeeng/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchangeeng-fs-v0.0.1) | The English-named edition of Color Change |
| [JK Color Change From Point Fs](colorchangefrompoint/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchangefrompoint-fs-v0.0.1) | Replaces pixels matching the color picked at a given point |
| [JK Color Change Fs](colorchange/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchange-fs-v0.0.1) | Replaces up to 8 specified colors with other colors |
| [JK Color Change Simple Fs](colorchangesimple/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchangesimple-fs-v0.0.1) | Replaces one specified color with another |
| [JK Color Key Fs](colorkey/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorkey-fs-v0.0.1) | Makes the specified color transparent |
| [JK Color Mat Key Fs](colormatkey/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colormatkey-fs-v0.0.1) | Keeps only pixels close to up to 4 specified colors |
| [JK Color Threshold Fs](colorthreshold/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorthreshold-fs-v0.0.1) | Snaps the masked area to the nearest of a fixed set of colors |
| [JK Create Alpha Fs](createalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/createalpha-fs-v0.0.1) | Builds the alpha channel from brightness (black becomes transparent) |
| [JK Edge Blur Fs](edgeblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/edgeblur-fs-v0.0.1) | Shrinks the alpha inward and then blurs it |
| [JK Edge Line Fs](edgeline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/edgeline-fs-v0.0.1) | Outlines a target color wherever a sample color is nearby |
| [JK Edge Line Hi Fs](edgelinehi/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/edgelinehi-fs-v0.0.1) | Casts rays from a target color and paints where they hit a sample color |
| [JK Exps Colors Fs](expscolors/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/expscolors-fs-v0.0.1) | A parameter sample that just lists 10 colors |
| [JK Exps Pos Fs](expspos/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/expspos-fs-v0.0.1) | A parameter sample that just lists 10 points |
| [JK Extract Edge Fs](extractedge/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/extractedge-fs-v0.0.1) | Extracts edge strength from steps in lightness |
| [JK Extract Hi Fs](extracthi/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/extracthi-fs-v0.0.1) | Multiplies the alpha by a chosen component through a threshold curve |
| [JK Fill Color Fs](fillcolor/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/fillcolor-fs-v0.0.1) | Fills the RGB channels with a specified color |
| [JK Filter Fs](filter/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/filter-fs-v0.0.1) | A glow workshop stacking up to 4 extract-dilate-blur-composite passes |
| [JK Flare Fs](flare/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/flare-fs-v0.0.1) | Creates light bleeding out of the bright areas |
| [JK Frame Fs](frame/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/frame-fs-v0.0.1) | Draws a centered frame and veils the outside with a translucent color |
| [JK Fs SS Frame](fsssframe/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/fsssframe-fs-v0.0.1) | A twin of F's Frame (identical source) |
| [JK Gray To Colorize Fs](graytocolorize/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/graytocolorize-fs-v0.0.1) | Reduces luminance to 5 steps and paints each step its own color |
| [JK Gray To Countour Line Fs](graytocountourline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/graytocountourline-fs-v0.0.1) | Rounds luminance into a given number of gray steps |
| [JK gray To Wave Line Fs](graytowaveline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/graytowaveline-fs-v0.0.1) | Draws brightness as the height of a line graph |
| [JK Graytone To Colorize Fs](graytonetocolorize/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/graytonetocolorize-fs-v0.0.1) | Colorizes a chosen channel's gray through a 9-stop gradient |
| [JK Guide Frame Fs](guideframe/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/guideframe-fs-v0.0.1) | Draws a rectangular guide frame defined by two points |
| [JK HLS Reverse Fs](hlsreverse/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/hlsreverse-fs-v0.0.1) | Inverts hue, lightness and saturation individually in HLS |
| [JK Inner Flare Fs](innerflare/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/innerflare-fs-v0.0.1) | Bleeds light inward along the shape of the image |
| [JK Inner Flare Rot Fs](innerflarerot/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/innerflarerot-fs-v0.0.1) | The directional edition of Inner Flare |
| [JK Iro Tore Adjacent Fs](irotoreadjacent/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/irotoreadjacent-fs-v0.0.1) | Recolors only the line art touching a specified adjacent color |
| [JK Iro Tore Fs](irotore/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/irotore-fs-v0.0.1) | Erases cel line art and fills it in from the surrounding paint |
| [JK Line Detection Fs](linedetection/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/linedetection-fs-v0.0.1) | Finds line-like pixels and paints them a specified color |
| [JK Line Trace Fs](linetrace/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/linetrace-fs-v0.0.1) | Traces lines using a per-hue threshold for each color family |
| [JK Main Line Repaint Fs](mainlinerepaint/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/mainlinerepaint-fs-v0.0.1) | Erases the main lines and repaints them from the fills on either side |
| [JK Main Line Repaint Old Fs](mainlinerepaintold/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/mainlinerepaintold-fs-v0.0.1) | The older main-line eraser, using a different method |
| [JK Mask From RGB Fs](maskfromrgb/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maskfromrgb-fs-v0.0.1) | Moves one channel into the alpha and fills the RGB with white |
| [JK Mask From RGB Multi Fs](maskfromrgbmulti/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maskfromrgbmulti-fs-v0.0.1) | Adds the strength of each of R, G, B, Y, M and C into the alpha |
| [JK Max Blur Fs](maxblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maxblur-fs-v0.0.1) | Applies dilate (or erode), blur and an alpha curve in one pass |
| [JK Max Fast Fs](maxfast/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maxfast-fs-v0.0.1) | Dilates or erodes at a speed independent of radius (van Herk / Gil-Werman) |
| [JK Max Fs](max/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/max-fs-v0.0.1) | Dilates or erodes with a maximum/minimum filter |
| [JK Max Kasumi Fs](maxkasumi/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maxkasumi-fs-v0.0.1) | Nudges each pixel toward a brighter neighbor |
| [JK Mosaic Fs](mosaic/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/mosaic-fs-v0.0.1) | Fills each size x size block with its average color |
| [JK Nami Garasu Fs](namigarasu/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/namigarasu-fs-v0.0.1) | Distorts the image as if seen through frosted glass |
| [JK Noise Hi Lo Alpha Fs](noisehiloalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/noisehiloalpha-fs-v0.0.1) | Adds separate noise to the alpha's shadows, midtones and highlights |
| [JK Noise Hi Lo RGB Fs](noisehilorgb/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/noisehilorgb-fs-v0.0.1) | Adds separate noise to the shadows, midtones and highlights |
| [JK Optical Diffusion Fs](opticaldiffusion/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/opticaldiffusion-fs-v0.0.1) | Optical diffusion: extracts light, blurs it and composites it back |
| [JK Out Line Fs](outline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/outline-fs-v0.0.1) | Turns the difference against an eroded alpha into an outline band |
| [JK Paint Fs](paint/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/paint-fs-v0.0.1) | Flood-fills the connected area of one color from a given point |
| [JK Paint Mult Point Fs](paintmultpoint/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/paintmultpoint-fs-v0.0.1) | The multi-point edition of Paint (up to 8 point-and-color pairs) |
| [JK Paper Plane Get Weight Fs](paperplanegetweight/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/paperplanegetweight-fs-v0.0.1) | Writes paper-plane weight data into the RGB channels |
| [JK Paper Plane Weight Fs](paperplaneweight/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/paperplaneweight-fs-v0.0.1) | Reads back the embedded weights and shows their total and center of mass |
| [JK Pixel Extend Fs](pixelextend/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/pixelextend-fs-v0.0.1) | Stretches the target color outward in each of 8 directions |
| [JK Pixel Replace Fs](pixelreplace/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/pixelreplace-fs-v0.0.1) | Applies 24 target-to-replacement color pairs in order |
| [JK Pixel Selector Fs](pixelselector/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/pixelselector-fs-v0.0.1) | Keeps only pixels of the specified color |
| [JK Posterization 8bit Fs](posterization8bit/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/posterization8bit-fs-v0.0.1) | Quantizes each channel into a number of levels |
| [JK Premultiply Fs](premultiply/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/premultiply-fs-v0.0.1) | Converts between premultiplied and straight alpha |
| [JK Random Line Noise Fs](randomlinenoise/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/randomlinenoise-fs-v0.0.1) | Scatters short bars at random and adds noise to the RGB beneath them |
| [JK Random Mosaic 2nd Fs](randommosaic2nd/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/randommosaic2nd-fs-v0.0.1) | Divides into cells and mosaics each one with a given probability |
| [JK Random Mosaic Fs](randommosaic/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/randommosaic-fs-v0.0.1) | Scatters rectangular mosaics with per-frame randomness |
| [JK Random Shift Fs](randomshift/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/randomshift-fs-v0.0.1) | Repeatedly re-pastes rectangular slices elsewhere |
| [JK Rgb To Alpha Fs](rgbtoalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/rgbtoalpha-fs-v0.0.1) | Moves luminance into the alpha and fills the RGB with a color |
| [JK RGBA Control Fs](rgbacontrol/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/rgbacontrol-fs-v0.0.1) | Raises or lowers R, G, B and A individually |
| [JK Rim Fill Fs](rimfill/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/rimfill-fs-v0.0.1) | Fills the transparent pixels around the image to add a rim |
| [JK Scanline Fs](scanline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/scanline-fs-v0.0.1) | Lays out scanline bands with separate settings for odd and even rows |
| [JK Screen Shake Dir Fs](screenshakedir/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/screenshakedir-fs-v0.0.1) | Shifts the whole frame by a random direction and amount |
| [JK Screen Shake MM Fs](screenshakemm/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/screenshakemm-fs-v0.0.1) | Shifts the whole frame by a random direction and amount |
| [JK Select Color Fs](selectcolor/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/selectcolor-fs-v0.0.1) | Makes pixels matching any of 8 target colors transparent |
| [JK Selected Blur Fs](selectedblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/selectedblur-fs-v0.0.1) | Blurs only the areas of up to 8 specified colors |
| [JK Shine Fs](shine/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/shine-fs-v0.0.1) | Draws light rays radiating outward from a given point |
| [JK Shine Parallel Fs](shineparallel/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/shineparallel-fs-v0.0.1) | The parallel edition of Shine |
| [JK Shitsu Kan Blur Fs](shitsukanblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/shitsukanblur-fs-v0.0.1) | Adds directional light to flat fills to give them texture |
| [JK Smoke Threshold Fs](smokethreshold/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/smokethreshold-fs-v0.0.1) | Binarizes the alpha and paints the result in 4 luminance bands |
| [JK Spark Fs](spark/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/spark-fs-v0.0.1) | Draws a jagged lightning bolt between two points |
| [JK Spark Grand Fs](sparkgrand/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparkgrand-fs-v0.0.1) | Draws a thick bolt from a point to somewhere along a line |
| [JK Spark Mult Fs](sparkmult/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparkmult-fs-v0.0.1) | Draws a jagged polyline through up to 7 points |
| [JK Spark Ring Fs](sparkring/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparkring-fs-v0.0.1) | Links points on an ellipse with jagged lines to form a ring |
| [JK Spark Storm Fs](sparkstorm/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparkstorm-fs-v0.0.1) | A twin of Spark (drawn the same way) |
| [JK Spark Test Fs](sparktest/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparktest-fs-v0.0.1) | A test harness for the Spark drawing parts |
| [JK Sputtering Alpha Fs](sputteringalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sputteringalpha-fs-v0.0.1) | Scatters blobs over the alpha's edge and interior at separate rates |
| [JK Sputtering Circle Fs](sputteringcircle/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sputteringcircle-fs-v0.0.1) | Scatters blobs inside a circle or ellipse around a point |
| [JK Sputtering Rect Fs](sputteringrect/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sputteringrect-fs-v0.0.1) | Layers paint-spatter blobs inside a rectangular area |
| [JK Sputtering Splash Fs](sputteringsplash/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sputteringsplash-fs-v0.0.1) | Splashes paint-like blobs outward from the alpha's edge |
| [JK Star Colorful Fs](starcolorful/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/starcolorful-fs-v0.0.1) | Extends cross and diagonal rays from the edges of bright areas |
| [JK Star Fs](star/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/star-fs-v0.0.1) | A cross filter extending 4-way rays from bright areas |
| [JK Target Grad Fs](targetgrad/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/targetgrad-fs-v0.0.1) | Draws a linear gradient into the alpha across a target's bounding box |
| [JK Target Grad Radical Fs](targetgradradical/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/targetgradradical-fs-v0.0.1) | Draws an elliptical gradient into the alpha around a target |
| [JK Thin Fs](thin/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/thin-fs-v0.0.1) | Thins the main lines of cel-style artwork |
| [JK To Gray Fs](togray/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/togray-fs-v0.0.1) | Blends the original with gray built by one of 8 methods |
| [JK Tone Fs](tone/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/tone-fs-v0.0.1) | Blends in a color interpolated from shadows, midtones and highlights |
| [JK Toner Fs](toner/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/toner-fs-v0.0.1) | A twin of Tone (identical source) |
| [JK Touch Draw Center Fs](touchdrawcenter/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/touchdrawcenter-fs-v0.0.1) | Draws a border of short strokes from the frame's edges inward |
| [JK Touch Draw Fs](touchdraw/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/touchdraw-fs-v0.0.1) | Draws many short strokes from a subject's edge toward the center |
| [JK Touch Draw Straght Fs](touchdrawstraght/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/touchdrawstraght-fs-v0.0.1) | Draws many short strokes from a subject's edge in a given direction |
| [JK Unmult KNSW Fs](unmultknsw/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/unmultknsw-fs-v0.0.1) | Premultiplies, then puts the maximum of RGB back into the alpha |
| [JK Unmult RG Fs](unmultrg/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/unmultrg-fs-v0.0.1) | Premultiplies, then puts the maximum of RGB back into the alpha |
| [JK Used Color List Fs](usedcolorlist/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/usedcolorlist-fs-v0.0.1) | Counts the colors in use and lays them out in cells by frequency |
| [JK Video Grid Fs](videogrid/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/videogrid-fs-v0.0.1) | Draws a 3D-looking grid by lighting and shading the cell edges |
| [JK Video Line 2nd Fs](videoline2nd/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/videoline2nd-fs-v0.0.1) | Overlays scanline stripes to brighten or darken |
| [JK Video Line Fs](videoline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/videoline-fs-v0.0.1) | Overlays scanline stripes to brighten or darken |
| [JK White In Out Fs](whiteinout/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/whiteinout-fs-v0.0.1) | Fades out to white, or back in from it |
| [JK Yuv Control Fs](yuvcontrol/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/yuvcontrol-fs-v0.0.1) | Raises or lowers Y, U and V individually in YUV |

Click a plug-in name for its parameters. Click a version for that version's files (Windows: `.aex`, macOS: `.plugin.zip`).

## Install

Download from the [latest release](https://github.com/JK-Plugins/fs-rs/releases/latest). Quit After Effects first.

- **Windows**: run `JK-Plugins-Fs-<date>-windows-setup.exe`. If "Windows protected your PC" appears, click "More info" → "Run anyway" (not signed).
  Plug-ins go into `C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\JK-Plugins`.
  Uninstall from Settings → Apps → Installed apps → **JK-Plugins Fs (After Effects)**.
  Without the installer: unzip `JK-Plugins-Fs-<date>-windows.zip` and copy the `JK-Plugins` folder into `MediaCore`.
- **macOS**: open `JK-Plugins-Fs-<date>-macos.pkg`. If it is blocked, click "Open Anyway" in System Settings → Privacy & Security (not signed).
  Plug-ins go into `/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/JK-Plugins`.

To update, just run the newer installer.

## Prerequisites

### **LLVM**

Download and install from  
https://github.com/llvm/llvm-project/releases

and you need to set `LIBCLANG_PATH` to the path of the `bin` directory in the LLVM installation.

For example, if you installed LLVM to `/path/to/llvm`, you would set:

```bash
# macOS
export LIBCLANG_PATH=/path/to/llvm/bin
# Windows (PowerShell)
setx LIBCLANG_PATH "C:\path\to\llvm\bin"
```

and, you need to set `PATH` to include the LLVM binaries:

```bash
# macOS
export PATH=/path/to/llvm/bin:$PATH
" # Windows (PowerShell)
setx PATH "C:\path\to\llvm\bin;$env:PATH"
# or, if you use LLVM Installer, you can set it optionally
```

### **Adobe After Effects SDK (May 2023)**

Download from https://console.adobe.io/downloads/ae
Place the SDK folder anywhere you like, then set:

```bash
# macOS
export AESDK_ROOT=/path/to/AfterEffects_SDK

# Windows (PowerShell)
setx AESDK_ROOT "C:\path\to\AfterEffects_SDK"
```

### **Rust**

Install Rust using [rustup](https://rustup.rs/).

### **cargo-jk**

We are implementing our original build tool.

Install it with:

```bash
cargo install --git https://github.com/JK-Plugins/cargo-jk
```

## Build

Run the following command to build the project:

```bash
cd pixelselector
cargo jk install
# this is the same as `cargo jk build && cargo jk mv [plugin_path]`
```

## Release

Actions → **Release** → **Run workflow** builds all plug-ins and publishes a release
(same mechanism as [JK-Plugins](https://github.com/JK-Plugins/JK-Plugins); settings in `.github/release.json`).

- Tag: today's date in JST, `vYYYY.MM.DD` (`.2`, `.3`, … for later releases on the same day)
- Assets: Windows installer and zip, macOS pkg, and per-plug-in `.aex` / `.plugin.zip`
- A per-plug-in archive release `<crate>-v<version>` is created only for new versions
- Versions live only in each `Cargo.toml` (`build.rs` reads them; minor/patch must be 0–15 for After Effects)
- The release stops if a plug-in changed without a version bump, or if the versions in the README plug-in lists are out of date
