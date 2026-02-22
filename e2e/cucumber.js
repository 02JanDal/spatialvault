module.exports = {
    default: {
        requireModule: ["tsx"],
        require: ["support/**/*.ts", "steps/**/*.ts"],
        paths: ["features/**/*.feature"],
        format: ["progress-bar"],
    },
    ci: {
        requireModule: ["tsx"],
        require: ["support/**/*.ts", "steps/**/*.ts"],
        paths: ["features/**/*.feature"],
        format: [
            "summary",
            "message:reports/messages.ndjson",
            "html:reports/e2e-report.html",
            "./support/github-annotations-formatter.ts",
        ],
        publish: true,
        publishQuiet: true,
    },
};
