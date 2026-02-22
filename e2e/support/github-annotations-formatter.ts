import * as path from "node:path";
import * as core from "@actions/core";
import {Formatter, formatterHelpers} from "@cucumber/cucumber";
import {TestStepResultStatus} from "@cucumber/messages";

const {parseTestCaseAttempt, isFailure, getUsage} = formatterHelpers;

export default class GitHubAnnotationsFormatter extends Formatter {
    constructor(options: ConstructorParameters<typeof Formatter>[0]) {
        super(options);
        options.eventBroadcaster.on(
            "envelope",
            (envelope: { testRunFinished?: { success: boolean } }) => {
                if (envelope.testRunFinished) {
                    this.logAnnotations();
                    this.logUnusedStepWarnings();
                    this.logJobSummary(envelope.testRunFinished.success);
                }
            },
        );
    }

    private logAnnotations() {
        const attempts = this.eventDataCollector.getTestCaseAttempts();
        for (const attempt of attempts) {
            if (!isFailure(attempt.worstTestStepResult, attempt.willBeRetried)) continue;

            const parsed = parseTestCaseAttempt({
                testCaseAttempt: attempt,
                snippetBuilder: this.snippetBuilder,
                supportCodeLibrary: this.supportCodeLibrary,
            });

            const failedStep = parsed.testSteps.find(
                (s: { result: { status: TestStepResultStatus } }) =>
                    s.result.status === TestStepResultStatus.FAILED,
            );

            const file = path.relative(this.cwd, path.resolve(this.cwd, attempt.pickle.uri));
            const startLine =
                (failedStep as { sourceLocation?: { line: number } })?.sourceLocation?.line ??
                (parsed.testCase as { sourceLocation?: { line: number } })?.sourceLocation?.line ??
                1;

            const message =
                (failedStep as { result: { message?: string } })?.result.message ??
                attempt.worstTestStepResult.message ??
                "Test failed";

            core.error(message, {
                file,
                startLine,
                title: attempt.pickle.name,
            });
        }
    }

    private getUnusedSteps() {
        const usage = getUsage({
            stepDefinitions: this.supportCodeLibrary.stepDefinitions,
            eventDataCollector: this.eventDataCollector,
        });
        return usage.filter((step) => step.matches.length === 0);
    }

    private logUnusedStepWarnings() {
        for (const step of this.getUnusedSteps()) {
            const file = path.relative(this.cwd, path.resolve(this.cwd, step.uri));
            core.warning(`Unused step definition: ${step.pattern}`, {
                file,
                startLine: step.line,
            });
        }
    }

    private logJobSummary(success: boolean) {
        const counts: Record<string, number> = {
            PASSED: 0,
            FAILED: 0,
            SKIPPED: 0,
            PENDING: 0,
            UNDEFINED: 0,
            AMBIGUOUS: 0,
        };

        const attempts = this.eventDataCollector.getTestCaseAttempts();
        for (const attempt of attempts) {
            const status = TestStepResultStatus[attempt.worstTestStepResult.status];
            if (status in counts) {
                counts[status]++;
            }
        }

        const total = attempts.length;
        const unusedSteps = this.getUnusedSteps().length;

        core.summary
            .addHeading("E2E Test Results", 2)
            .addTable([
                [
                    {data: "Metric", header: true},
                    {data: "Count", header: true},
                ],
                ["Total tests", `${total}`],
                ["Passed", `${counts.PASSED}`],
                ["Failed", `${counts.FAILED}`],
                ["Skipped", `${counts.SKIPPED}`],
                ["Pending", `${counts.PENDING}`],
                ["Undefined", `${counts.UNDEFINED}`],
                ["Unused step definitions", `${unusedSteps}`],
            ])
            .write();
    }
}
