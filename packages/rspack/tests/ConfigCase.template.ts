"use strict";
import { rspack } from "../src";
import assert from "assert";
import {
	ensureRspackConfigNotExist,
	ensureWebpackConfigExist,
	isValidTestCaseDir
} from "./utils";

const path = require("path");
const fs = require("graceful-fs");
const vm = require("vm");
const { URL, pathToFileURL, fileURLToPath, parse } = require("url");
const rimraf = require("rimraf");
const checkArrayExpectation = require("./checkArrayExpectation");
const createLazyTestEnv = require("./helpers/createLazyTestEnv");
const deprecationTracking = require("./helpers/deprecationTracking");
const FakeDocument = require("./helpers/FakeDocument");
const CurrentScript = require("./helpers/CurrentScript");

const prepareOptions = require("./helpers/prepareOptions");
// const { parseResource } = require("../lib/util/identifier");
const captureStdio = require("./helpers/captureStdio");
const asModule = require("./helpers/asModule");
const filterInfraStructureErrors = require("./helpers/infrastructureLogErrors");
// fake define
const define = function (...args) {
	const factory = args.pop();
	factory();
};

const casesPath = path.join(__dirname, "configCases");
const categories = fs
	.readdirSync(casesPath)
	.filter(isValidTestCaseDir)
	.map(cat => {
		return {
			name: cat,
			tests: fs
				.readdirSync(path.join(casesPath, cat))
				.filter(isValidTestCaseDir)
				.filter(folder =>
					fs.lstatSync(path.join(casesPath, cat, folder)).isDirectory()
				)
				.sort()
		};
	});

const createLogger = appendTarget => {
	return {
		log: l => appendTarget.push(l),
		debug: l => appendTarget.push(l),
		trace: l => appendTarget.push(l),
		info: l => appendTarget.push(l),
		warn: console.warn.bind(console),
		error: console.error.bind(console),
		logTime: () => {},
		group: () => {},
		groupCollapsed: () => {},
		groupEnd: () => {},
		profile: () => {},
		profileEnd: () => {},
		clear: () => {},
		status: () => {}
	};
};

export const describeCases = config => {
	describe(config.name, () => {
		let stderr;
		beforeEach(() => {
			stderr = captureStdio(process.stderr, true);
		});
		afterEach(() => {
			stderr.restore();
		});

		for (const category of categories) {
			// eslint-disable-next-line no-loop-func
			describe(category.name, () => {
				for (const testName of category.tests) {
					// eslint-disable-next-line no-loop-func
					const testDirectory = path.join(casesPath, category.name, testName);

					ensureRspackConfigNotExist(testDirectory);
					ensureWebpackConfigExist(testDirectory);

					const configFile = path.join(testDirectory, "webpack.config.js");
					describe(testName, function () {
						const filterPath = path.join(testDirectory, "test.filter.js");
						if (
							fs.existsSync(filterPath) &&
							(typeof require(filterPath)() === "string" ||
								!require(filterPath)())
						) {
							describe.skip(testName, () => {
								it("filtered", () => {});
							});
							return;
						}
						const infraStructureLog = [];
						const outBaseDir = path.join(__dirname, "js");
						const testSubPath = path.join(config.name, category.name, testName);
						// const outputDirectory = path.join(outBaseDir, testSubPath);
						const outputDirectory = path.join(testDirectory, "dist");
						const cacheDirectory = path.join(outBaseDir, ".cache", testSubPath);
						let options, optionsArr, testConfig;

						beforeAll(() => {
							options = {};
							if (fs.existsSync(configFile)) {
								options = prepareOptions(require(configFile), {
									testPath: outputDirectory
								});
							}
							optionsArr = [].concat(options);
							optionsArr.forEach((options, idx) => {
								if (!options.context) options.context = testDirectory;
								if (!options.mode) options.mode = "development";
								// if (!options.optimization) options.optimization = {};
								// if (options.optimization.minimize === undefined)
								//     options.optimization.minimize = false;
								// if (options.optimization.minimizer === undefined) {
								//     options.optimization.minimizer = [
								//         new (require("terser-webpack-plugin"))({
								//             parallel: false
								//         })
								//     ];
								// }
								if (!options.entry)
									options.entry = {
										main: "./"
									};
								if (!options.target) options.target = "node";
								if (!options.output) options.output = {};
								if (!options.output.path) options.output.path = outputDirectory;
								// if (typeof options.output.pathinfo === "undefined")
								//     options.output.pathinfo = true;
								// if (!options.output.filename)
								//     options.output.filename =
								//         "bundle" +
								//         idx +
								//         (options.experiments && options.experiments.outputModule
								//             ? ".mjs"
								//             : ".js");
								// if (config.cache) {
								//     options.cache = {
								//         cacheDirectory,
								//         name: `config-${idx}`,
								//         ...config.cache
								//     };
								//     options.infrastructureLogging = {
								//         debug: true,
								//         console: createLogger(infraStructureLog)
								//     };
								// }
								// if (!options.snapshot) options.snapshot = {};
								// if (!options.snapshot.managedPaths) {
								//     options.snapshot.managedPaths = [
								//         path.resolve(__dirname, "../node_modules")
								//     ];
								// }
							});
							testConfig = {
								findBundle: function (i, options) {
									// const ext = path.extname(
									//     parse(options.output.filename).path
									// );
									// if (
									//     fs.existsSync(
									//         path.join(options.output.path, "bundle" + i + ext)
									//     )
									// ) {
									//     return "./bundle" + i + ext;
									// }
									return "./main.js";
								}
								// timeout: 30000
							};
							try {
								// try to load a test file
								testConfig = Object.assign(
									testConfig,
									require(path.join(testDirectory, "test.config.js"))
								);
							} catch (e) {
								// ignored
							}
							// if (testConfig.timeout) setDefaultTimeout(testConfig.timeout);
						});
						afterAll(() => {
							// cleanup
							options = undefined;
							optionsArr = undefined;
							testConfig = undefined;
						});
						beforeAll(() => {
							rimraf.sync(cacheDirectory);
						});
						const handleFatalError = (err, done) => {
							const fakeStats = {
								errors: [
									{
										message: err.message,
										stack: err.stack
									}
								]
							};
							if (
								checkArrayExpectation(
									testDirectory,
									fakeStats,
									"error",
									"Error",
									done
								)
							) {
								return;
							}
							// Wait for uncaught errors to occur
							setTimeout(done, 200);
							return;
						};
						it(`${testName} should compile`, _done => {
							// console.info("running:", testName);
							// console.time(testName);
							const done = (...args: any[]) => {
								// console.timeEnd(testName);
								return _done(...args);
							};
							rimraf.sync(outputDirectory);
							fs.mkdirSync(outputDirectory, { recursive: true });
							infraStructureLog.length = 0;
							const deprecationTracker = deprecationTracking.start();
							const onCompiled = (err, stats) => {
								const deprecations = deprecationTracker();
								if (err) return handleFatalError(err, done);
								const statOptions = {
									preset: "verbose",
									colors: false
								};
								fs.mkdirSync(outputDirectory, { recursive: true });
								// fs.writeFileSync(
								//     path.join(outputDirectory, "stats.txt"),
								//     stats.toString(statOptions),
								//     "utf-8"
								// );
								const jsonStats = stats.toJson({
									errorDetails: true
								});
								// fs.writeFileSync(
								//     path.join(outputDirectory, "stats.json"),
								//     JSON.stringify(jsonStats, null, 2),
								//     "utf-8"
								// );
								// error case not expect error
								if (category.name === "errors") {
									assert(jsonStats.errors!.length > 0);
								} else if (category.name === "warnings") {
									assert(jsonStats.warnings!.length > 0);
								} else {
									if (jsonStats.errors!.length > 0) {
										console.log(
											`case: ${category.name} ${testName}\nerrors:\n`,
											`${jsonStats.errors!.map(x => x.message).join("\n")}`
										);
									}
									assert(
										jsonStats.errors!.length === 0,
										`${JSON.stringify(jsonStats.errors, null, 2)}`
									);
								}
								// if (
								//     checkArrayExpectation(
								//         testDirectory,
								//         jsonStats,
								//         "error",
								//         "Error",
								//         done
								//     )
								// ) {
								//     return;
								// }
								// if (
								//     checkArrayExpectation(
								//         testDirectory,
								//         jsonStats,
								//         "warning",
								//         "Warning",
								//         done
								//     )
								// ) {
								//     return;
								// }
								const infrastructureLogging = stderr.toString();
								if (infrastructureLogging) {
									return done(
										new Error(
											"Errors/Warnings during build:\n" + infrastructureLogging
										)
									);
								}
								if (
									checkArrayExpectation(
										testDirectory,
										{ deprecations },
										"deprecation",
										"Deprecation",
										done
									)
								) {
									return;
								}
								const infrastructureLogErrors = filterInfraStructureErrors(
									infraStructureLog,
									{
										run: 3,
										options
									}
								);
								if (
									infrastructureLogErrors.length &&
									checkArrayExpectation(
										testDirectory,
										{ infrastructureLogs: infrastructureLogErrors },
										"infrastructureLog",
										"infrastructure-log",
										"InfrastructureLog",
										done
									)
								) {
									return;
								}

								let filesCount = 0;
								if (testConfig.noTests) return process.nextTick(done);
								if (testConfig.beforeExecute) testConfig.beforeExecute();
								const results = [];
								for (let i = 0; i < optionsArr.length; i++) {
									const options = optionsArr[i];
									const bundlePath = testConfig.findBundle(i, optionsArr[i]);
									if (bundlePath) {
										filesCount++;
										const document = new FakeDocument(outputDirectory);
										const globalContext = {
											console: console,
											expect: expect,
											setTimeout: setTimeout,
											clearTimeout: clearTimeout,
											document,
											getComputedStyle:
												document.getComputedStyle.bind(document),
											location: {
												href: "https://test.cases/path/index.html",
												origin: "https://test.cases",
												toString() {
													return "https://test.cases/path/index.html";
												}
											}
										};

										const requireCache = Object.create(null);
										const esmCache = new Map();
										const esmIdentifier = `${category.name}-${testName}-${i}`;
										const baseModuleScope = {
											console: console,
											it: _it,
											beforeEach: _beforeEach,
											afterEach: _afterEach,
											expect,
											jest,
											__STATS__: jsonStats,
											nsObj: m => {
												Object.defineProperty(m, Symbol.toStringTag, {
													value: "Module"
												});
												return m;
											}
										};

										let runInNewContext = false;
										if (
											options.target === "web" ||
											options.target === "webworker"
										) {
											// @ts-ignore
											baseModuleScope.window = globalContext;
											// @ts-ignore
											baseModuleScope.self = globalContext;
											// @ts-ignore
											baseModuleScope.URL = URL;
											// @ts-ignore
											baseModuleScope.Worker =
												require("./helpers/createFakeWorker")({
													outputDirectory
												});
											runInNewContext = true;
										}
										if (testConfig.moduleScope) {
											testConfig.moduleScope(baseModuleScope);
										}
										const esmContext = vm.createContext(baseModuleScope, {
											name: "context for esm"
										});

										// eslint-disable-next-line no-loop-func
										const _require = (
											currentDirectory,
											options,
											module,
											esmMode,
											parentModule
										) => {
											if (testConfig === undefined) {
												throw new Error(
													`_require(${module}) called after all tests from ${category.name} ${testName} have completed`
												);
											}
											if (Array.isArray(module) || /^\.\.?\//.test(module)) {
												let content;
												let p;
												let subPath = "";
												if (Array.isArray(module)) {
													p = path.join(currentDirectory, ".array-require.js");
													content = `module.exports = (${module
														.map(arg => {
															return `require(${JSON.stringify(`./${arg}`)})`;
														})
														.join(", ")});`;
												} else {
													p = path.join(currentDirectory, module);
													content = fs.readFileSync(p, "utf-8");
													const lastSlash = module.lastIndexOf("/");
													let firstSlash = module.indexOf("/");

													if (lastSlash !== -1 && firstSlash !== lastSlash) {
														if (firstSlash !== -1) {
															let next = module.indexOf("/", firstSlash + 1);
															let dir = module.slice(firstSlash + 1, next);

															while (dir === ".") {
																firstSlash = next;
																next = module.indexOf("/", firstSlash + 1);
																dir = module.slice(firstSlash + 1, next);
															}
														}

														subPath = module.slice(
															firstSlash + 1,
															lastSlash + 1
														);
													}
												}
												const isModule =
													// p.endsWith(".mjs") &&
													options.experiments &&
													options.experiments.outputModule;

												if (isModule) {
													if (!vm.SourceTextModule)
														throw new Error(
															"Running this test requires '--experimental-vm-modules'.\nRun with 'node --experimental-vm-modules node_modules/jest-cli/bin/jest'."
														);
													let esm = esmCache.get(p);
													if (!esm) {
														esm = new vm.SourceTextModule(content, {
															identifier: esmIdentifier + "-" + p,
															url: pathToFileURL(p).href + "?" + esmIdentifier,
															context: esmContext,
															initializeImportMeta: (meta, module) => {
																meta.url = pathToFileURL(p).href;
															},
															importModuleDynamically: async (
																specifier,
																module
															) => {
																const result = await _require(
																	path.dirname(p),
																	options,
																	specifier,
																	"evaluated",
																	module
																);
																return await asModule(result, module.context);
															}
														});
														esmCache.set(p, esm);
													}
													if (esmMode === "unlinked") return esm;
													return (async () => {
														await esm.link(
															async (specifier, referencingModule) => {
																return await asModule(
																	await _require(
																		path.dirname(
																			referencingModule.identifier
																				? referencingModule.identifier.slice(
																						esmIdentifier.length + 1
																				  )
																				: fileURLToPath(referencingModule.url)
																		),
																		options,
																		specifier,
																		"unlinked",
																		referencingModule
																	),
																	referencingModule.context,
																	true
																);
															}
														);
														// node.js 10 needs instantiate
														if (esm.instantiate) esm.instantiate();
														await esm.evaluate();
														if (esmMode === "evaluated") return esm;
														const ns = esm.namespace;
														return ns.default && ns.default instanceof Promise
															? ns.default
															: ns;
													})();
												} else {
													if (p in requireCache) {
														return requireCache[p].exports;
													}
													const m = {
														exports: {}
													};
													requireCache[p] = m;
													const moduleScope = {
														...baseModuleScope,
														require: _require.bind(
															null,
															path.dirname(p),
															options
														),
														importScripts: url => {
															expect(url).toMatch(
																/^https:\/\/test\.cases\/path\//
															);
															// @ts-ignore
															_require(
																outputDirectory,
																options,
																`.${url.slice(
																	"https://test.cases/path".length
																)}`
															);
														},
														module: m,
														exports: m.exports,
														__dirname: path.dirname(p),
														__filename: p,
														_globalAssign: { expect },
														define
													};
													if (testConfig.moduleScope) {
														testConfig.moduleScope(moduleScope);
													}
													if (!runInNewContext)
														content = `Object.assign(global, _globalAssign);\n ${content}`;
													const args = Object.keys(moduleScope);
													const argValues = args.map(arg => moduleScope[arg]);
													const code = `(function(${args.join(
														", "
													)}) {${content}\n})`;

													let oldCurrentScript = document.currentScript;
													document.currentScript = new CurrentScript(subPath);
													const fn = runInNewContext
														? vm.runInNewContext(code, globalContext, p)
														: vm.runInThisContext(code, p);
													fn.call(
														testConfig.nonEsmThis
															? testConfig.nonEsmThis(module)
															: m.exports,
														...argValues
													);
													document.currentScript = oldCurrentScript;
													return m.exports;
												}
											} else if (
												testConfig.modules &&
												module in testConfig.modules
											) {
												return testConfig.modules[module];
											} else {
												return require(module.startsWith("node:")
													? module.slice(5)
													: module);
											}
										};
										if (Array.isArray(bundlePath)) {
											for (const bundlePathItem of bundlePath) {
												results.push(
													// @ts-ignore
													_require(
														outputDirectory,
														options,
														"./" + bundlePathItem
													)
												);
											}
										} else {
											results.push(
												// @ts-ignore
												_require(outputDirectory, options, bundlePath)
											);
										}
									}
								}
								// give a free pass to compilation that generated an error
								if (
									!jsonStats.errors.length &&
									filesCount !== optionsArr.length
								) {
									return done(
										new Error(
											"Should have found at least one bundle file per webpack config"
										)
									);
								}
								Promise.all(results)
									.then(() => {
										if (testConfig.afterExecute) testConfig.afterExecute();
										for (const key of Object.keys(global)) {
											if (key.includes("webpack")) delete global[key];
										}
										if (getNumberOfTests() < filesCount) {
											return done(new Error("No tests exported by test case"));
										}
										done();
									})
									.catch(done);
							};
							rspack(optionsArr, onCompiled);
						} /* 30000 */);

						const {
							it: _it,
							beforeEach: _beforeEach,
							afterEach: _afterEach,
							setDefaultTimeout,
							getNumberOfTests
						} = createLazyTestEnv(10000);
					});
				}
			});
		}
	});
};
