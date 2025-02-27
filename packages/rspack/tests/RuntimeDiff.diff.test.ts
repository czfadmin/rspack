import path from "path";
import fs from "fs-extra";
import {
	Tester,
	DiffProcessor,
	IDiffProcessorOptions,
	ECompareResultType,
	TModuleCompareResult
} from "@rspack/test-tools";
import rimraf from "rimraf";
import { isValidTestCaseDir } from "./utils";

const DEFAULT_CASE_CONFIG: IDiffProcessorOptions = {
	webpackPath: require.resolve("webpack"),
	rspackPath: require.resolve("@rspack/core"),
	files: ["bundle.js"],
	ignorePropertyQuotationMark: true,
	ignoreModuleId: true,
	ignoreModuleArguments: true
};

type TFileCompareResult = {
	modules: TModuleCompareResult[];
	runtimeModules: TModuleCompareResult[];
};

function createDiffCase(name: string, src: string, dist: string) {
	const caseConfigFile = path.join(src, "test.config.js");
	if (!fs.existsSync(caseConfigFile)) {
		return;
	}
	const caseConfig: IDiffProcessorOptions = Object.assign(
		{},
		DEFAULT_CASE_CONFIG,
		require(caseConfigFile)
	);

	const [processor, compareMap] = createDiffProcessor(caseConfig);
	const tester = new Tester({
		name,
		src,
		dist,
		steps: [processor]
	});

	describe(name, () => {
		beforeAll(async () => {
			rimraf.sync(dist);
			await tester.prepare();
		});

		do {
			const prefix = `[${name}][${tester.step + 1}]:`;
			describe(`${prefix}build`, () => {
				beforeAll(async () => {
					await tester.compile();
				});
				checkBundleFiles(
					"webpack",
					path.join(dist, "webpack"),
					caseConfig.files!
				);
				checkBundleFiles(
					"rspack",
					path.join(dist, "rspack"),
					caseConfig.files!
				);
			});
			describe(`${prefix}check`, () => {
				beforeAll(async () => {
					compareMap.clear();
					await tester.check();
				});
				for (let file of caseConfig.files!) {
					describe(`Comparing "${file}"`, () => {
						let moduleResults: TModuleCompareResult[] = [];
						let runtimeResults: TModuleCompareResult[] = [];
						beforeAll(() => {
							const fileResult = compareMap.get(file);
							if (!fileResult) {
								throw new Error(`File ${file} has no results`);
							}
							moduleResults = fileResult.modules;
							runtimeResults = fileResult.runtimeModules;
						});
						if (caseConfig.modules) {
							checkCompareResults("modules", () => moduleResults);
						}
						if (caseConfig.runtimeModules) {
							checkCompareResults("runtime modules", () => runtimeResults);
						}
					});
				}
			});
		} while (tester.next());

		afterAll(async () => {
			await tester.resume();
		});
	});
}

function createDiffProcessor(config: IDiffProcessorOptions) {
	const fileCompareMap: Map<string, TFileCompareResult> = new Map();
	const createCompareResultHandler = (type: keyof TFileCompareResult) => {
		return (file: string, results: TModuleCompareResult[]) => {
			const fileResult = fileCompareMap.get(file) || {
				modules: [],
				runtimeModules: []
			};
			fileResult[type] = results;
			fileCompareMap.set(file, fileResult);
		};
	};

	const processor = new DiffProcessor({
		webpackPath: config.webpackPath,
		rspackPath: config.rspackPath,
		files: config.files,
		modules: config.modules,
		runtimeModules: config.runtimeModules,
		ignoreModuleId: config.ignoreModuleId ?? true,
		ignoreModuleArguments: config.ignoreModuleArguments ?? true,
		ignorePropertyQuotationMark: config.ignorePropertyQuotationMark ?? true,
		onCompareModules: createCompareResultHandler("modules"),
		onCompareRuntimeModules: createCompareResultHandler("runtimeModules")
	});

	return [processor, fileCompareMap] as [
		DiffProcessor,
		Map<string, TFileCompareResult>
	];
}

function checkBundleFiles(name: string, dist: string, files: string[]) {
	describe(`Checking ${name} dist files`, () => {
		for (let file of files) {
			it(`${name}: ${file} should be generated`, () => {
				expect(fs.existsSync(path.join(dist, file))).toBeTruthy();
			});
		}
	});
}

function checkCompareResults(
	name: string,
	getResults: () => TModuleCompareResult[]
) {
	describe(`Comparing ${name}`, () => {
		it("should not miss any module", () => {
			expect(
				getResults()
					.filter(i => i.type === ECompareResultType.Missing)
					.map(i => i.name)
			).toEqual([]);
		});
		it("should not have any respack-only module", () => {
			expect(
				getResults()
					.filter(i => i.type === ECompareResultType.OnlySource)
					.map(i => i.name)
			).toEqual([]);
		});
		it("should not have any webpack-only module", () => {
			expect(
				getResults()
					.filter(i => i.type === ECompareResultType.OnlyDist)
					.map(i => i.name)
			).toEqual([]);
		});
		it(`all modules should be the same`, () => {
			for (let result of getResults().filter(
				i => i.type === ECompareResultType.Different
			)) {
				console.log(`${result.name}:\n${result.detail}`);
			}
			expect(
				getResults().every(i => i.type === ECompareResultType.Same)
			).toEqual(true);
		});
	});
}

const caseDir: string = path.resolve(__dirname, "runtimeDiffCases");
const tempDir: string = path.resolve(__dirname, "js");
const cases: string[] = fs.readdirSync(caseDir).filter(isValidTestCaseDir);

describe(`RuntimeDiffCases`, () => {
	for (let name of cases) {
		const src = path.join(caseDir, name);
		const dist = path.join(tempDir, `runtime-diff/${name}`);
		createDiffCase(name, src, dist);
	}
});
