const path = require('path');
const MiniCssExtractPlugin = require('mini-css-extract-plugin');
const CssMinimizerPlugin = require("css-minimizer-webpack-plugin");

module.exports = (env, argv) => {
    return {
        entry: {
            'svg': ['./ts/svg.ts', './css/svg.css'],
            'svg-shadow': './css/svg-shadow.css',
        },
        devtool: (argv.mode === 'development') ? 'inline-source-map' : 'source-map',
        module: {
            rules: [
                {
                    test: /\.tsx?$/,
                    use: 'ts-loader',
                    exclude: /node_modules/
                },
                {
                    test: /\.css$/i,
                    use: [MiniCssExtractPlugin.loader, 'css-loader']
                }
            ]
        },
        resolve: {
            extensions: ['.tsx', '.ts', '.mjs', '.js']
        },
        plugins: [
            new MiniCssExtractPlugin({
                // filename: '../css/svg.css'
                filename: '[name].css'
            })
        ],
        output: {
            path: path.resolve(__dirname, 'dist'),
            filename: '[name].js',
        },
        optimization: {
            minimize: (argv.mode === 'production'),
            minimizer: [
                new CssMinimizerPlugin(), '...'
            ],
        },
    }
}
