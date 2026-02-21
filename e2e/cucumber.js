module.exports = {
  default: {
    requireModule: ["tsx"],
    require: ["support/**/*.ts", "steps/**/*.ts"],
    paths: ["features/**/*.feature"],
    format: ["progress-bar"],
  },
};
