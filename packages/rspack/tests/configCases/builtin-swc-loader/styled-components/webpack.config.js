/** @type {import('@rspack/core').Configuration}*/
module.exports = {
	resolve: {
		alias: {
			"@xstyled/styled-components": "styled-components"
		}
	},
	module: {
		rules: [
			{
				test: /\.jsx?$/,
				loader: "builtin:swc-loader",
				options: {
					jsc: {
						parser: {
							syntax: "ecmascript",
							jsx: true
						}
					},
					rspackExperiments: {
						styledComponents: {
							displayName: true,
							ssr: true,
							fileName: true,
							meaninglessFileNames: ["index", "styles"],
							namespace: "rspack-test",
							topLevelImportPaths: [
								"@xstyled/styled-components",
								"@xstyled/styled-components/*"
							],
							transpileTemplateLiterals: true,
							minify: true,
							pure: true,
							cssProps: true
						}
					}
				}
			}
		]
	}
};
