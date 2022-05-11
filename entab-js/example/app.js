/*
  TODO:
    - Bar chart if Y axis is None? Density plot?
    - URL params to allow sharing URL-loaded files?
    - Disclaimer? Link to Github
    - Handle dates in entab-js
    - Panning/zooming the graph
    - "Play" data as sound/music
    - Allow downloading a picture

    - Handle dates in CSV parsing
    - Add a method to StateMetadata to expose bounds so they don't always need to be calculated here
 */

const lang = (window.navigator.userLanguage || window.navigator.language).toLowerCase();

function translate(text, key = "") {
  let msg;
  if (lang === "en" || lang === "en-us") {
    msg = {
      "Colour": "Color",
    }[key || text];
  } else if (lang.startsWith("es")) {
    msg = {
      "Enter a URL": "Introduce una URL",
      "Loading file…": "Cargando archivo…",
      "Calculating bounds…": "Cálculo de límites…",
      "File type": "Tipo de archivo",
      "X axis": "Eje X",
      "Y axis": "Eje Y",
      "Colour": "Color",
      "color-none": "Sin",
      "Download": "Descargar un",
      "file-drop-msg": "Arrastre y suelte un archivo en esta ventana o:",
      "Pick a file": "Escoge un archivo",
      "See a sample file": "Ver un archivo de ejemplo",
      "Close file": "Cerrar el archivo",
      "Open settings panel": "Abra el panel de configuración",
      "Close settings panel": "Cerrar el panel de configuración",
      "error-msg": "Hubo un problema al analizar el archivo.",
      "Palette": "Esquema del color",
    }[key || text];
  } else if (lang.startsWith("fr")) {
    msg = {
      "Enter a URL": "Entrez une URL",
      "Loading file…": "Fichier de chargement…",
      "Calculating bounds…": "Calcul des limites…",
      "File type": "Type de fichier",
      "X axis": "Axe X",
      "Y axis": "Axe Y",
      "Colour": "Couleur",
      "color-none": "Sans",
      "Download": "Télécharger un",
      "file-drop-msg": "Glisser et déposez un fichier dans cette fenêtre ou:",
      "Pick a file": "Choisir un fichier",
      "See a sample file": "Voir un exemple de fichier",
      "Close file": "Fermez le fichier",
      "Open settings panel": "Ouvrir le panneau paramètres",
      "Close settings panel": "Fermer le panneau paramètres",
      "error-msg": "Un problème est survenu lors de l'analyse du fichier.",
      "Palette": "Schéma de couleur",
    }[key || text];
  } else if (lang === "zh" || lang === "zh-cn") {
    msg = {
      "Enter a URL": "写网址",
      "Loading file…": "加载文件……",
      "Calculating bounds…": "计算界限……",
      "File type": "文件类型",
      "X axis": "X轴",
      "Y axis": "Y轴",
      "Colour": "颜色",
      "color-none": "没有",
      "Download": "下载",
      "file-drop-msg": "文件拖放到此窗口里或者",
      "Pick a file": "选择文件",
      "See a sample file": "看例子",
      "Close file": "关文件",
      "Open settings panel": "开设置面板",
      "Close settings panel": "关设置面板",
      "error-msg": "解析文件出现问题。",
      "Palette": "配色",
    }[key || text];
  } else if (lang.startsWith("zh")) {
    msg = {
      "Enter a URL": "寫網址",
      "Loading file…": "加載文件……",
      "Calculating bounds…": "計算界限……",
      "File type": "文件類型",
      "X axis": "X軸",
      "Y axis": "Y軸",
      "Colour": "顏色",
      "color-none": "沒有",
      "Download": "下載",
      "file-drop-msg": "文件拖放到此窗口里或者",
      "Pick a file": "選擇文件",
      "See a sample file": "看例子",
      "Close file": "關文件",
      "Open settings panel": "開設置面板",
      "Close settings panel": "關設置面板",
      "error-msg": "解析文件出現問題。",
      "Palette": "配色",
    }[key || text];
  }
  if (!msg) msg = text;

  return msg;
}

let curProcess = null;
const app = PetiteVue.createApp({
  graph: {},
  filename: "",
  showOverlay: false,
  statusType: "file",
  statusMessage: "",
  translate,
  mounted() {
    window.addEventListener("resize", this.render);
  },
  fileDropped(event) {
    this.processFile(event.dataTransfer.files[0]);
  },
  clickFileInput() {
    document.getElementById("file-input").click();
  },
  filePicked(event) {
    this.processFile(event.target.files[0]);
  },
  clickChooseUrl() {
    const url = prompt(translate("Enter a URL"));
    this.chooseUrl(url);
  },
  clickChooseSample() {
    this.chooseUrl("https://raw.githubusercontent.com/plotly/datasets/master/earthquakes-23k.csv").then(() => {
      this.graph.xaxis = "Longitude";
      this.graph.yaxis = "Latitude";
      this.graph.caxis = "Magnitude";
    });
  },
  chooseUrl(url) {
    this.statusType = "";
    this.statusMessage = translate("Loading file…");
    return fetch(url).then(response => {
      response.name = url.split("/").slice(-1)[0];
      return this.processFile(response);
    }).catch(e => {
      console.error(e);
      this.statusType = "error";
      // TODO: translate
      this.statusMessage = "A network error occured.";
    });
  },
  closeFile() {
    clearTimeout(curProcess);
    this.statusType = "file";
    this.statusMessage = "";
    this.filename = "";
    this.graph = {};
  },
  async processFile(file) {
    // TODO: double check that Reader is loaded?
    clearTimeout(curProcess);
    let parserName = undefined;
    if (file.name.endsWith(".csv")) {
      parserName = "csv";
    } else if (file.name.endsWith(".csv")) {
      parserName = "tsv";
    }
    try {
      const buffer = new Uint8Array(await file.arrayBuffer());
      const reader = new Reader(buffer, parserName);
      // TODO: use nPoints for progress bar on canvas?
      this.statusType = "";
      this.statusMessage = translate("Calculating bounds…");
      this.filename = file.name;
      const [bounds, columns, nPoints] = await calculateBounds(reader);
      let xaxis, yaxis, caxis;
      switch (reader.parser) {
        case "fasta":
          [xaxis, yaxis, caxis] = ["length(sequence)", "gc(sequence)", ""];
          break;
        case "fastq":
          [xaxis, yaxis, caxis] = ["gc(sequence)", "average(quality)","length(sequence)"];
          break;
        case "sam":
        case "bam":
          [xaxis, yaxis, caxis] = ["mapq", "average(quality)","length(sequence)"];
        default:
          [xaxis, yaxis, caxis] = columns;
      };
      this.graph = {
        parser: reader.parser,
        bounds,
        buffer,
        columns,
        xaxis,
        yaxis,
        caxis,
        cmap: "Turbo",
      };
      this.statusMessage = "";
    } catch (e) {
      console.error(e);
      this.statusType = "error";
      this.statusMessage = e;
    }
  },
  toggleOverlay(status) {
    this.showOverlay = status;
  },
  downloadTsv() {
    const delimiter = "\t";
    const quoteChar = '"';

    const reader = new Reader(this.graph.buffer, this.graph.parser);
    const tsv = [];

    const columns = reader.headers;
    tsv.push(columns.join(delimiter) + "\n");

    function quote(text) {
      if (typeof text !== "string") return text;
      if (text.match(delimiter)) {
        return quoteChar + text.replaceAll(quoteChar, quoteChar + quoteChar) + quoteChar;
      }
      return text;
    }

    for (const datum of reader) {
      tsv.push(columns.map(c => quote(datum[c])).join(delimiter) + "\n");
    }

    const blob = new Blob(tsv, { type: "text/tsv;charset=utf-8;" });
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    link.download = `${this.filename}.tsv`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  },
  render() {
    // basically a debounce
    curProcess = setTimeout(this.renderAsync, 200);
  },
  renderAsync() {
    const [leftMargin, rightMargin, bottomMargin, topMargin] = [50, 5, 5, 20];
    const pointRadius = 2;
    let chart = d3.select("svg");
    const height = chart.node().clientHeight - bottomMargin - topMargin;
    const width = chart.node().clientWidth - leftMargin - rightMargin;
    chart.selectAll("*").remove();
    chart = chart.append("g").attr("transform", `translate(${leftMargin},${bottomMargin})`);
    if (!this.graph.buffer) {
      return;
    }
    const xScale = d3.scaleLinear(this.graph.bounds[this.graph.xaxis].slice(1, 3), [0, width]);
    const yScale = d3.scaleLinear(this.graph.bounds[this.graph.yaxis].slice(1, 3), [height, 0]);
    const xTransform = d => xScale(this.graph.bounds[this.graph.xaxis][0](d));
    const yTransform = d => yScale(this.graph.bounds[this.graph.yaxis][0](d));
    const darkMode = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
    let color;
    if (this.graph.caxis) {
      const colorScheme = d3['interpolate' + this.graph.cmap];
      const cScale = d3.scaleSequential(this.graph.bounds[this.graph.caxis].slice(1, 3), colorScheme);
      color = d => cScale(this.graph.bounds[this.graph.caxis][0](d));
    } else if (darkMode) {
      color = d => "white";
    } else {
      color = d => "black";
    }

    chart.append("g")
         .attr("transform", `translate(0,${height})`)
         .call(d3.axisBottom(xScale))
         .classed("axis", true);
    chart.append("g")
         .call(d3.axisLeft(yScale))
         .classed("axis", true);

    /*
    // svg-based render for a few points
    const readerForSvg = new Reader(this.graph.buffer, this.graph.parser);
    chart.append("g")
         .selectAll("dot")
         .data(Array.from(readerForSvg))
         .enter()
         .append("circle")
         .attr("cx", xTransform)
         .attr("cy", yTransform)
         .attr("r", `${pointRadius}px`)
         .attr("fill", d => color(d));
    */

    // canvas-based render if there are tons of points
    let reader;
    try {
      reader = new Reader(this.graph.buffer, this.graph.parser);
    } catch (e) {
      console.error(e);
      this.statusType = "error";
      this.statusMessage = e;
      return;
    }
    const foreignObject = chart.append("foreignObject").attr("height", height).attr("width", width);
    const canvas = foreignObject.node().appendChild(document.createElement("canvas"));
    canvas.height = height;
    canvas.width = width;
    const cxt = canvas.getContext("2d");

    const processChunk = () => {
      try {
        const chunk = reader_chunk(reader);
        for (const datum of chunk) {
          cxt.beginPath();
          cxt.rect(
            xTransform(datum) - pointRadius,
            yTransform(datum) - pointRadius,
            2 * pointRadius,
            2 * pointRadius
          );
          cxt.fillStyle = color(datum);
          cxt.fill();
          cxt.closePath();
        }
        if (!reader.done) {
          curProcess = setTimeout(processChunk);
        }
      } catch (e) {
        console.error(e);
        this.statusType = "error";
        this.statusMessage = e;
      }
    };
    curProcess = setTimeout(processChunk);
  },
});

function* reader_chunk(reader) {
  let n_left = 2000;
  while (n_left--) {
    const value = reader.next();
    if (value.done) {
      reader.done = true;
      break;
    }
    yield value.value;
  }
}

const FUNCTIONS = {
  gc(str) {
    let gc = 0;
    let other = 0;
    for (const c of str.toUpperCase()) {
      switch (c) {
        case "G":
        case "C":
        case "S":
          gc++;
          break;
        case "A":
        case "T":
        case "U":
        case "W":
          other++;
          break;
        case "R":
        case "Y":
        case "K":
        case "M":
          gc += 0.5;
          other += 0.5;
          break;
        default:
      }
    }
    return gc / (gc + other);
  },
  avgQual(str) {
    let sum = 0;
    let n = 0;
    for (let i = 0, code = 0; code = str.charCodeAt(i); i++) {
      sum += code;
      n++;
    }
    return sum / n;
  },
};

async function calculateBounds(reader) {
  const datum = reader.next().value;
  const bounds = {};
  const columns = [];
  let nPoints = 0;
  for (const column of reader.headers) {
    const value = datum[column];
    // TODO: handle dates, booleans?
    if (typeof value === "string") {
      bounds[`length(${column})`] = [v => v[column].length, value.length, value.length];
      columns.push(`length(${column})`);
      if (column === "sequence") {
        bounds[`gc(${column})`] = [v => FUNCTIONS.gc(v[column]), FUNCTIONS.gc(value), FUNCTIONS.gc(value)];
        columns.push(`gc(${column})`);
      } else if (column === "quality") {
        bounds[`average(quality)`] = [v => FUNCTIONS.avgQual(v[column]), FUNCTIONS.avgQual(value), FUNCTIONS.avgQual(value)];
        columns.push(`average(quality)`);
      }
    } else {
      // it's a number
      bounds[column] = [v => v[column], value, value];
      columns.push(column);
    }
  }

  return new Promise((resolve, reject) => {
    const processChunk = () => {
      const chunk = reader_chunk(reader);
      for (const datum of chunk) {
        for (const column of Object.keys(bounds)) {
          const value = bounds[column][0](datum);
          bounds[column][1] = Math.min(bounds[column][1], value);
          bounds[column][2] = Math.max(bounds[column][2], value);
        }
        nPoints++;
      }
      if (reader.done) {
        resolve([bounds, columns, nPoints]);
      } else {
        curProcess = setTimeout(processChunk);
      }
    };
    curProcess = setTimeout(processChunk);
  });
}

app.mount("#app");
