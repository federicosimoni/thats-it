# README assets

- `demo.gif` — the demo at the top of the README: a file dragged onto the widget, the ring
  opening, a conversion chosen, the result appearing. Keep it under ~5 MB, about 560 px
  wide (1120 px for Retina), 10 s or less.

  One way to make it: record the screen region with **Shift-Command-5** (Record Selected
  Portion), then convert the `.mov` with ffmpeg:

  ```sh
  ffmpeg -i demo.mov -vf "fps=15,scale=1120:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse" demo.gif
  ```

  Or drag the `.mov` onto That's it and pick **Convert to GIF**.
