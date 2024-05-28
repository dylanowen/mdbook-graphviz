const path = require('path');
const MiniCssExtractPlugin = require('mini-css-extract-plugin');
const CssMinimizerPlugin = require("css-minimizer-webpack-plugin");

module.exports = (env, argv) => {
    return {
        entry: ['./ts/index.ts', './css/index.css'],
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
                filename: 'svg.css'
            })
        ],
        output: {
            // path: path.resolve(__dirname, '../../sample/book/html/js'),
            path: path.resolve(__dirname, 'dist'),
            filename: 'svg.js',
        },
        optimization: {
            minimize: (argv.mode === 'production'),
            minimizer: [
                new CssMinimizerPlugin(), '...'
            ],
        },
    }
}
