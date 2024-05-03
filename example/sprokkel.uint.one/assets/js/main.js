(() => {
  // <stdin>
  var ColorScheme = {
    Light: "light",
    Dark: "dark"
  };
  function defaultColorScheme() {
    if (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) {
      return ColorScheme.Dark;
    } else {
      return ColorScheme.Light;
    }
  }
  function hydrateColorScheme() {
    let cs = localStorage.getItem("color-scheme");
    if (cs === null) {
      cs = defaultColorScheme();
    }
    if (cs === ColorScheme.Dark) {
      document.documentElement.classList.add("dark");
    }
  }
  function listenColorSchemeToggle() {
    document.querySelector("#color-scheme-toggle").onclick = () => {
      document.documentElement.classList.toggle("dark");
      if (document.documentElement.classList.contains("dark")) {
        localStorage.setItem("color-scheme", "dark");
      } else {
        localStorage.setItem("color-scheme", "light");
      }
    };
  }
  function main() {
    hydrateColorScheme();
    window.addEventListener("load", () => listenColorSchemeToggle());
  }
  main();
})();
